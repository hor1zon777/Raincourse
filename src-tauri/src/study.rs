use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use futures_util::stream::{self, StreamExt};

use crate::ai::quiz_runner;
use crate::api::client::RainClient;
use crate::error::AppError;
use crate::ws;

// 常量（对应原 Python config.py）
const VIDEO_COMPLETION_THRESHOLD: f64 = 0.99;
const LEARNING_RATE: i64 = 4;
const HEARTBEAT_BATCH_SIZE: i64 = 3;
// 每轮真实等待 = (LEARNING_RATE × HEARTBEAT_BATCH_SIZE) / 播放倍率（见 handle_video），
// 故不再使用固定 LOOP_SLEEP_INTERVAL。
const RETRY_SLEEP_INTERVAL: u64 = 5;
const PPT_DURATION_SPAN: u64 = 1;

#[derive(Clone, Serialize)]
pub struct StudyTaskEvent {
    pub index: usize,
    pub total: usize,
    pub name: String,
    pub type_str: String,
    pub status: String,
    pub progress: Option<f64>,
    pub message: Option<String>,
}

/// 前端展示的章节任务项
#[derive(Clone, Serialize)]
pub struct ChapterTask {
    pub index: usize,
    pub id: i64,
    pub name: String,
    pub leaf_type: i32,
    pub type_str: String,
}

fn emit_task(app: &AppHandle, evt: &StudyTaskEvent) {
    let _ = app.emit("study-task-update", evt.clone());
}

fn is_cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// 构建心跳数据包
///
/// `user_id` / `video_id` 必须能解析为正整数，否则雨课堂会拒绝心跳并可能触发风控。
/// 解析失败时返回错误，由上层中断当前任务。
/// 每心跳步进固定为 [`LEARNING_RATE`]；播放倍率由上层用「真实等待时间」体现，不改步进。
fn build_heartbeat(
    course_id: &str,
    video_id: &str,
    classroom_id: &str,
    sku_id: &str,
    user_id: &str,
    current_frame: i64,
) -> Result<Vec<Value>, AppError> {
    let user_id_num: i64 = user_id
        .parse()
        .map_err(|_| AppError::ApiError(format!("用户 ID 非法（'{}'），无法发送心跳", user_id)))?;
    if user_id_num <= 0 {
        return Err(AppError::ApiError(format!(
            "用户 ID 必须为正整数（实际 {}）",
            user_id_num
        )));
    }
    let video_id_num: i64 = video_id
        .parse()
        .map_err(|_| AppError::ApiError(format!("视频 ID 非法（'{}'），无法发送心跳", video_id)))?;

    let ts = chrono::Utc::now().timestamp_millis();
    let mut rng = rand::rng();
    let mut data = Vec::new();

    for i in 0..HEARTBEAT_BATCH_SIZE {
        let frame = current_frame + (i + 1) * LEARNING_RATE;
        let pg_suffix: String = (0..4)
            .map(|_| rng.random_range(b'a'..=b'z') as char)
            .collect();

        data.push(json!({
            "i": 5,
            "et": "loadeddata",
            "p": "web",
            "n": "ali-cdn.xuetangx.com",
            "lob": "cloud4",
            "cp": frame,
            "fp": 0, "tp": 0, "sp": 2,
            "ts": ts.to_string(),
            "u": user_id_num,
            "uip": "",
            "c": course_id,
            "v": video_id_num,
            "skuid": sku_id,
            "classroomid": classroom_id,
            "cc": video_id,
            "d": 4976.5,
            "pg": format!("{}_{}", video_id, pg_suffix),
            "sq": i + 1,
            "t": "video"
        }));
    }
    Ok(data)
}

/// 把播放倍率格式化为简洁字符串（1.0→"1"、1.5→"1.5"、2.0→"2"）。
fn format_rate(rate: f64) -> String {
    let s = format!("{:.2}", rate);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// 处理视频任务
async fn handle_video(
    app: &AppHandle,
    client: &RainClient,
    course_id: &str,
    leaf_id: &str,
    name: &str,
    evt: &mut StudyTaskEvent,
    cancel: &Arc<AtomicBool>,
    playback_rate: f64,
) -> Result<(), AppError> {
    evt.status = "running".into();
    evt.message = Some("获取视频信息...".into());
    emit_task(app, evt);

    let course_sign = client.get_course_sign(course_id).await?["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let leaf_info = client
        .get_leaf_info(leaf_id, course_id, &course_sign)
        .await?;
    let cid = leaf_info["data"]["course_id"].to_string().replace('"', "");
    let user_id = leaf_info["data"]["user_id"].to_string().replace('"', "");
    let sku_id = leaf_info["data"]["sku_id"].to_string().replace('"', "");

    // 关键字段校验：解析失败立即终止，避免用 0 / 空字符串发送心跳触发风控
    if user_id.is_empty() || user_id == "null" {
        return Err(AppError::ApiError(format!(
            "视频 '{}' 缺少 user_id，可能未登录或接口异常",
            name
        )));
    }
    if cid.is_empty() || cid == "null" {
        return Err(AppError::ApiError(format!(
            "视频 '{}' 缺少 course_id",
            name
        )));
    }

    // 检查是否已完成
    let status = client.get_status(leaf_id, course_id).await?;
    if status["data"].as_bool() == Some(true) {
        evt.status = "done".into();
        evt.message = Some("已完成，跳过".into());
        emit_task(app, evt);
        return Ok(());
    }

    // 获取初始进度
    let progress_resp = client
        .get_video_progress(leaf_id, &cid, course_id, &user_id)
        .await?;
    let mut rate = progress_resp
        .as_ref()
        .and_then(|r| r["data"][leaf_id]["rate"].as_f64())
        .unwrap_or(0.0);
    let mut video_frame = progress_resp
        .as_ref()
        .and_then(|r| r["data"][leaf_id]["watch_length"].as_i64())
        .unwrap_or(0);

    // 真实倍速：每轮推进固定 LEARNING_RATE×HEARTBEAT_BATCH_SIZE 秒视频，真实等待 =
    // 该推进秒数 ÷ 倍率，使后台检测到的观看速度≈所选倍率、观看时长≈视频时长÷倍率
    //（避免"秒级跑完整段视频"被判异常）。倍率越高等待越短，但不改变心跳步进。
    let advance_per_loop = (LEARNING_RATE * HEARTBEAT_BATCH_SIZE) as f64;
    let loop_sleep_secs = (advance_per_loop / playback_rate.max(0.1)).max(0.5);

    evt.progress = Some(rate);
    evt.message = Some(format!(
        "初始进度 {:.1}%，开始自动学习（{}x）...",
        rate * 100.0,
        format_rate(playback_rate)
    ));
    emit_task(app, evt);

    while rate < VIDEO_COMPLETION_THRESHOLD {
        if is_cancelled(cancel) {
            evt.status = "failed".into();
            evt.message = Some("已被用户停止".into());
            emit_task(app, evt);
            return Err(AppError::Cancelled);
        }

        let heartbeat =
            build_heartbeat(&cid, leaf_id, course_id, &sku_id, &user_id, video_frame)?;
        video_frame += LEARNING_RATE * HEARTBEAT_BATCH_SIZE;

        let resp_text = client.send_heartbeat(heartbeat).await?;

        // 检查频率限制
        if let Some(text) = &resp_text {
            if let Some(pos) = text.find("Expected available in ") {
                let after = &text[pos + 22..];
                if let Some(end) = after.find(" second") {
                    if let Ok(delay) = after[..end].trim().parse::<f64>() {
                        evt.message = Some(format!("触发频率限制，等待 {:.1}s...", delay));
                        emit_task(app, evt);
                        sleep(Duration::from_secs_f64(delay + 0.5)).await;
                    }
                }
            }
        }

        if is_cancelled(cancel) {
            evt.status = "failed".into();
            evt.message = Some("已被用户停止".into());
            emit_task(app, evt);
            return Err(AppError::Cancelled);
        }

        // 获取最新进度
        let latest = client
            .get_video_progress(leaf_id, &cid, course_id, &user_id)
            .await?;
        if let Some(r) = latest
            .as_ref()
            .and_then(|r| r["data"][leaf_id]["rate"].as_f64())
        {
            rate = r;
        } else {
            evt.message = Some(format!("未能获取进度，{}s后重试...", RETRY_SLEEP_INTERVAL));
            emit_task(app, evt);
            sleep(Duration::from_secs(RETRY_SLEEP_INTERVAL)).await;
            continue;
        }

        evt.progress = Some(rate);
        evt.message = Some(format!("学习进度: {:.1}%", rate * 100.0));
        emit_task(app, evt);

        // 真实倍速节流：按 loop_sleep_secs 真实等待，分片检查取消以便及时停止
        let mut remaining = loop_sleep_secs;
        while remaining > 0.0 {
            if is_cancelled(cancel) {
                break;
            }
            let chunk = remaining.min(1.0);
            sleep(Duration::from_secs_f64(chunk)).await;
            remaining -= chunk;
        }
    }

    evt.status = "done".into();
    evt.progress = Some(1.0);
    evt.message = Some(format!("视频 '{}' 学习完成", name));
    emit_task(app, evt);
    Ok(())
}

/// 处理公告任务
async fn handle_announcement(
    app: &AppHandle,
    client: &RainClient,
    course_id: &str,
    leaf_id: &str,
    evt: &mut StudyTaskEvent,
) -> Result<(), AppError> {
    evt.status = "running".into();
    evt.message = Some("正在浏览公告...".into());
    emit_task(app, evt);

    let course_sign_resp = client.get_course_sign(course_id).await?;
    let course_sign = course_sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let status = client.get_status(leaf_id, course_id).await?;
    if status["data"].as_bool() == Some(true) {
        evt.status = "done".into();
        evt.message = Some("已完成，跳过".into());
        emit_task(app, evt);
        return Ok(());
    }

    let leaf_info = client
        .get_leaf_info(leaf_id, course_id, &course_sign)
        .await?;
    let sku_id = leaf_info["data"]["sku_id"].to_string().replace('"', "");

    let ret = client
        .read_announcement(leaf_id, course_id, &sku_id)
        .await?;
    evt.status = "done".into();
    evt.message = Some(format!(
        "公告已浏览: {}",
        ret.get("success").unwrap_or(&json!(true))
    ));
    emit_task(app, evt);
    Ok(())
}

/// 处理 PPT 浏览任务
async fn handle_ppt_leaf(
    app: &AppHandle,
    client: &RainClient,
    course_id: &str,
    leaf_id: &str,
    evt: &mut StudyTaskEvent,
) -> Result<(), AppError> {
    evt.status = "running".into();
    evt.message = Some("获取课件信息...".into());
    emit_task(app, evt);

    let course_sign_resp = client.get_course_sign(course_id).await?;
    let course_sign = course_sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("");

    let leaf_info = client
        .get_leaf_info(leaf_id, course_id, course_sign)
        .await?;
    let user_id = leaf_info["data"]["user_id"].as_i64().unwrap_or(0);

    let page_count = leaf_info["data"]
        .get("content_info")
        .and_then(|c| c.get("page_num"))
        .and_then(|p| p.as_i64())
        .unwrap_or(10) as i32;

    evt.message = Some(format!("正在浏览课件 ({}页)...", page_count));
    emit_task(app, evt);

    match ws::ppt::view_ppt(leaf_id, user_id, page_count).await {
        Ok(_) => {
            evt.status = "done".into();
            evt.message = Some("课件浏览完成".into());
        }
        Err(e) => {
            evt.status = "done".into();
            evt.message = Some(format!("课件浏览完成 (WS: {})", e));
        }
    }
    emit_task(app, evt);
    sleep(Duration::from_secs(PPT_DURATION_SPAN)).await;
    Ok(())
}

/// 从章节数据中提取所有 leaf 任务
pub fn extract_tasks(chapter_data: &Value) -> Vec<(i64, String, i32)> {
    let mut tasks = Vec::new();
    let empty = vec![];
    let chapters = chapter_data["data"]["course_chapter"]
        .as_array()
        .unwrap_or(&empty);

    for chapter in chapters {
        let sections = chapter["section_leaf_list"].as_array().unwrap_or(&empty);
        for section in sections {
            if let Some(leaf_list) = section["leaf_list"].as_array() {
                for leaf in leaf_list {
                    let id = leaf["id"].as_i64().unwrap_or(0);
                    let name = leaf["name"].as_str().unwrap_or("未知任务").to_string();
                    let leaf_type = leaf["leaf_type"].as_i64().unwrap_or(-1) as i32;
                    if id > 0 {
                        tasks.push((id, name, leaf_type));
                    }
                }
            } else {
                let id = section["id"].as_i64().unwrap_or(0);
                let name = section["name"].as_str().unwrap_or("未知任务").to_string();
                let leaf_type = section["leaf_type"].as_i64().unwrap_or(-1) as i32;
                if id > 0 {
                    tasks.push((id, name, leaf_type));
                }
            }
        }
    }
    tasks
}

pub fn leaf_type_str(t: i32) -> &'static str {
    match t {
        0 => "视频",
        3 => "公告",
        4 => "讨论",
        6 => "测验/练习",
        _ => "未知",
    }
}

/// 处理单个章节任务（视频 / 公告 / 讨论 / 测验）。
///
/// 并行刷课时每个任务独立调用，**不**在此发 `study-stopped` / `study-complete`
/// （由 [`run_auto_study`] 在全部任务结束后按取消状态统一发一次）。被取消时静默返回。
#[allow(clippy::too_many_arguments)]
async fn process_task(
    app: AppHandle,
    client: RainClient,
    course_id: String,
    app_data_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    auto_answer_quiz: bool,
    playback_rate: f64,
    index: usize,
    total: usize,
    leaf_id: i64,
    name: String,
    leaf_type: i32,
) {
    if is_cancelled(&cancel) {
        return;
    }

    let mut evt = StudyTaskEvent {
        index,
        total,
        name: name.clone(),
        type_str: leaf_type_str(leaf_type).to_string(),
        status: "pending".into(),
        progress: None,
        message: None,
    };
    let lid = leaf_id.to_string();

    match leaf_type {
        0 => {
            if let Err(e) = handle_video(
                &app,
                &client,
                &course_id,
                &lid,
                &name,
                &mut evt,
                &cancel,
                playback_rate,
            )
            .await
            {
                // 取消时 handle_video 已把该任务标记为「已被用户停止」，这里静默返回
                if is_cancelled(&cancel) {
                    return;
                }
                evt.status = "failed".into();
                evt.message = Some(format!("错误: {}", e));
                emit_task(&app, &evt);
            }
        }
        3 => {
            if let Err(e) = handle_announcement(&app, &client, &course_id, &lid, &mut evt).await {
                evt.status = "failed".into();
                evt.message = Some(format!("错误: {}", e));
                emit_task(&app, &evt);
            }
        }
        4 => {
            evt.status = "skipped".into();
            evt.message = Some("讨论任务已跳过".into());
            emit_task(&app, &evt);
        }
        6 => {
            if auto_answer_quiz {
                evt.status = "running".into();
                evt.message = Some("自动答题中…".into());
                emit_task(&app, &evt);

                // 用 move 闭包构造全新 StudyTaskEvent 上报逐题进度，
                // 避免与外层 evt 的可变借用冲突；进度文案带「题库/AI」来源标记。
                let app_cb = app.clone();
                let name_cb = name.clone();
                let mut on_progress = move |p: quiz_runner::QuizProgress| {
                    let prefix = if p.status == "preparing" {
                        String::new()
                    } else {
                        format!("[{}/{}] ", p.index, p.total)
                    };
                    let src = match p.source {
                        Some("local") => "题库 ",
                        Some("ai") => "AI ",
                        _ => "",
                    };
                    let msg = format!("{}{}{}", prefix, src, p.message.unwrap_or_default());
                    let progress = if p.status == "preparing" || p.total == 0 {
                        None
                    } else {
                        Some(p.index as f64 / p.total as f64)
                    };
                    let sub = StudyTaskEvent {
                        index,
                        total,
                        name: name_cb.clone(),
                        type_str: "测验/练习".to_string(),
                        status: "running".into(),
                        progress,
                        message: Some(msg),
                    };
                    let _ = app_cb.emit("study-task-update", sub);
                };

                let res = quiz_runner::run_quiz_answer(
                    &client,
                    &app_data_dir,
                    &course_id,
                    &lid,
                    false, // 刷课流程真实提交
                    &cancel,
                    &mut on_progress,
                )
                .await;
                drop(on_progress);

                // run_quiz_answer 取消时静默 break 返回 Ok(部分结果)，这里判取消后静默返回
                if is_cancelled(&cancel) {
                    return;
                }
                match res {
                    Ok(r) => {
                        evt.status = "done".into();
                        evt.progress = Some(1.0);
                        evt.message = Some(format!(
                            "答题完成：提交 {}，正确 {}，已答跳过 {}，失败 {}，跳过 {}",
                            r.submitted, r.correct, r.already_answered, r.failed, r.skipped
                        ));
                        emit_task(&app, &evt);
                    }
                    Err(e) => {
                        evt.status = "failed".into();
                        evt.message = Some(format!("自动答题错误: {}", e));
                        emit_task(&app, &evt);
                    }
                }
            } else {
                evt.status = "skipped".into();
                evt.message = Some("测验任务已跳过（未开启刷课自动答题）".into());
                emit_task(&app, &evt);
            }
        }
        _ => {
            evt.status = "skipped".into();
            evt.message = Some(format!("未知任务类型 {}", leaf_type));
            emit_task(&app, &evt);
        }
    }
}

/// 主入口：自动刷课（支持取消 + 自定义任务 + 多任务并行）
///
/// 进入时重置 `cancel`；外层 commands 层按 `course_id` 维护 cancel 标志，
/// 多课程并发时互不干扰。`auto_answer_quiz=true` 时对测验/练习复用
/// [`quiz_runner::run_quiz_answer`] 真实提交（题库优先 / AI 兜底）。
/// `playback_rate` 为视频刷课播放倍率（真实倍速：耗时≈视频时长÷倍率）。
/// `concurrency` 为并行观看任务数（1=串行；越大总耗时越短，但同账号并发心跳越多、风控风险越高）。
#[allow(clippy::too_many_arguments)]
pub async fn run_auto_study(
    app: AppHandle,
    client: RainClient,
    course_id: String,
    cancel: Arc<AtomicBool>,
    task_ids: Option<Vec<i64>>,
    app_data_dir: PathBuf,
    auto_answer_quiz: bool,
    playback_rate: f64,
    concurrency: usize,
) -> Result<(), AppError> {
    // 重置取消标志（同一课程二次启动场景）
    cancel.store(false, Ordering::Relaxed);

    // 获取章节数据
    let sign_resp = client.get_course_sign(&course_id).await?;
    let course_sign = sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let chapter_data = client.get_all_chapter(&course_id, &course_sign).await?;

    let all_tasks = extract_tasks(&chapter_data);

    // 如果指定了 task_ids，只保留选中的任务
    let tasks: Vec<(i64, String, i32)> = match &task_ids {
        Some(ids) => all_tasks
            .into_iter()
            .filter(|(id, _, _)| ids.contains(id))
            .collect(),
        None => all_tasks,
    };
    let total = tasks.len();

    if total == 0 {
        let _ = app.emit("study-complete", json!({}));
        return Ok(());
    }

    // 并行驱动：最多 `concurrency` 个任务同时进行，其余排队（bounded 并发）。
    // 每个任务独立处理与上报进度；前端按 index 归并、并发显示多个「处理中」。
    let concurrency = concurrency.max(1);
    stream::iter(tasks.into_iter().enumerate())
        .for_each_concurrent(concurrency, |(i, (leaf_id, name, leaf_type))| {
            // 每个任务克隆一份共享句柄（RainClient 共享同一 Arc<Jar>，AppHandle/Arc 轻量克隆）
            let app = app.clone();
            let client = client.clone();
            let course_id = course_id.clone();
            let app_data_dir = app_data_dir.clone();
            let cancel = cancel.clone();
            async move {
                process_task(
                    app,
                    client,
                    course_id,
                    app_data_dir,
                    cancel,
                    auto_answer_quiz,
                    playback_rate,
                    i + 1,
                    total,
                    leaf_id,
                    name,
                    leaf_type,
                )
                .await;
            }
        })
        .await;

    if is_cancelled(&cancel) {
        let _ = app.emit("study-stopped", json!({"message": "已停止刷课"}));
    } else {
        let _ = app.emit("study-complete", json!({}));
    }
    Ok(())
}
