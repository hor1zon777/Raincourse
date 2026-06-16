use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::client::RainClient;
use crate::error::AppError;
use crate::models::course::Course;
use crate::models::exam::{ExportResult, Ppt, Work, WorkStatus};
use crate::session::manager as sess;
use crate::storage::json_store;
use crate::study::{self, ChapterTask};
use crate::util::json_str_or_num;
use crate::{excel, ws};

use serde::Serialize;

use crate::ai::config::{self as ai_config, AiConfig};
use crate::ai::encode;
use crate::ai::quiz_runner::{self, QuizAnswerResult};

/// 全局应用状态。
///
/// - `client`: 用 parking_lot::RwLock 包裹。读用 `.read().clone()` 立刻释放，
///   切换/退出账号时 write lock 替换为新 RainClient → 旧 cookie 彻底清空。
/// - `username`: 用 parking_lot Mutex，不会中毒。
/// - `study_cancels`: 按 `course_id` 维度的取消令牌，避免多课程并发刷课时互相影响。
pub struct AppState {
    pub client: RwLock<RainClient>,
    pub username: Mutex<Option<String>>,
    pub study_cancels: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            client: RwLock::new(RainClient::new()),
            username: Mutex::new(None),
            study_cancels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 借出当前 RainClient 的 Clone（含同一份 Arc<Jar>）。
    /// 持锁时间仅一次 Arc clone，不跨 await。
    fn snapshot_client(&self) -> RainClient {
        self.client.read().clone()
    }

    /// 为指定课程获取（或创建）取消标志。
    pub fn get_or_create_cancel(&self, course_id: &str) -> Arc<AtomicBool> {
        let mut map = self.study_cancels.lock();
        map.entry(course_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    /// 取消指定课程的刷课；返回是否找到对应任务。
    pub fn cancel_course(&self, course_id: &str) -> bool {
        let map = self.study_cancels.lock();
        if let Some(flag) = map.get(course_id) {
            flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 取消所有正在进行的刷课。
    pub fn cancel_all(&self) -> usize {
        let map = self.study_cancels.lock();
        for flag in map.values() {
            flag.store(true, Ordering::Relaxed);
        }
        map.len()
    }

    /// 用全新的 RainClient 替换当前 client（旧 jar 整个 drop）。
    /// 用于"切换账户/扫码登录前"，避免上一个账号或匿名 init 留下的 cookies
    /// 与即将加载的 cookies 混合，导致服务端识别为异常 session。
    pub fn reset_client(&self) -> RainClient {
        let new_client = RainClient::new();
        *self.client.write() = new_client.clone();
        new_client
    }
}

/// 统一获取 app_data_dir，错误传播而不是 panic。
fn app_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Config(format!("无法获取应用数据目录: {}", e)))
}

/// 把 JSON 值取成 f64，兼容「数字或数字字符串」；无法解析返回 0.0。
///
/// 雨课堂成绩字段大多是数字（如 `user_score: 2.7`），个别是字符串
/// （如 `user_final_score: "82.48"`），统一在此兜底，避免类型差异导致丢分。
fn json_f64(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

// ========== 认证 Commands ==========

#[tauri::command]
pub async fn init_client(state: State<'_, AppState>) -> Result<(), AppError> {
    let client = state.snapshot_client();
    client.init().await
}

#[tauri::command]
pub async fn start_qr_login(app: AppHandle, state: State<'_, AppState>) -> Result<Value, AppError> {
    // 入口重置 client：丢弃 init_client 等任何遗留 cookies，让本次扫码登录
    // 从一个完全干净的 jar 开始，避免新 session 混入旧匿名 cookie
    let client = state.reset_client();

    // 先做一次 /web 初始化拿到 csrftoken 等必要的初始 cookie
    if let Err(e) = client.init().await {
        log::warn!("扫码登录前 init 失败（继续尝试）: {}", e);
    }

    let result = ws::login::start_qr_login(app.clone()).await?;

    // 用登录结果换取 session
    client.post_web_login(result.user_id, &result.auth).await?;

    // 获取用户信息（直接返回给前端，省去前端二次 invoke）
    let user_info = client.get_user_info().await?;

    // 严格校验响应：拿不到 name 视为登录流程异常，不再静默兜底为 "unknown"
    // 这能让前端立即看到真实的错误，而不是看见 "登录成功" 然后卡住
    let name = extract_name_from_userinfo(&user_info).ok_or_else(|| {
        log::error!(
            "扫码登录后 userinfo 响应缺少 name 字段，原始响应: {}",
            user_info
        );
        AppError::ApiError(
            "扫码登录成功，但服务端未返回用户信息（响应格式异常），请重试".to_string(),
        )
    })?;

    *state.username.lock() = Some(name.clone());

    // 保存 session：按域分组 dump，覆盖 yuketang/xuetangx 等所有相关域
    let app_data_dir = app_data_dir(&app)?;
    let cookies_by_domain = client.dump_cookies_by_domain();
    if cookies_by_domain.is_empty() {
        log::warn!("登录后未发现任何 cookie，可能登录流程异常");
    } else {
        log::info!(
            "扫码登录完成，已保存 {} 个域的 cookies: {:?}",
            cookies_by_domain.len(),
            cookies_by_domain.keys().collect::<Vec<_>>()
        );
    }
    sess::save_session(&app_data_dir, &name, cookies_by_domain)?;

    Ok(user_info)
}

/// 从 userinfo 响应里抽取用户名。
///
/// 雨课堂的 /v2/api/web/userinfo 在不同账号下可能返回：
/// - `{ data: [{ name: "..." }] }`
/// - `{ data: { name: "..." } }`
/// - 字段名偶尔是 `Name`
fn extract_name_from_userinfo(user_info: &Value) -> Option<String> {
    extract_field_from_userinfo(user_info, &["name", "Name", "username"])
}

/// 通用字段提取：兼容 data 为 array 或单 object 的两种结构。
fn extract_field_from_userinfo(user_info: &Value, keys: &[&str]) -> Option<String> {
    fn pick(v: &Value, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(s) = v.get(*key).and_then(|x| x.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
        None
    }

    let data = user_info.get("data")?;
    if let Some(arr) = data.as_array() {
        if let Some(first) = arr.first() {
            return pick(first, keys);
        }
    }
    if data.is_object() {
        return pick(data, keys);
    }
    None
}

#[tauri::command]
pub async fn login_with_session(
    app: AppHandle,
    state: State<'_, AppState>,
    username: String,
) -> Result<Value, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let session_data = sess::load_session(&app_data_dir, &username)?;

    // **关键修复**：登录前重置 client，确保 jar 中只有这个账号的 cookies。
    // 否则 init_client 拿到的匿名 cookie / 上一个用户的残留会与存储 cookies 混合，
    // 表现为"切换账号后再次登录提示会话已过期"。
    let client = state.reset_client();

    let by_domain = session_data.effective_cookies_by_domain();
    log::info!(
        "登录 {}：从磁盘恢复 {} 个域的 cookies",
        username,
        by_domain.len()
    );
    client.load_cookies_by_domain(&by_domain);

    *state.username.lock() = Some(username.clone());

    // 获取用户信息验证 session 有效性
    let user_info = match client.get_user_info().await {
        Ok(v) => v,
        Err(e) => {
            // 网络/JSON 错误也要清掉污染的 jar，避免下次重试基于错误 state
            *state.username.lock() = None;
            state.reset_client();
            return Err(e);
        }
    };

    // 检查响应是否表明 session 已过期。
    //
    // 与 start_qr_login 使用同一套提取器：只要能拿到 name 就视为有效响应。
    // 避免旧版只看 `data.as_array().is_empty()` 时，对 `{ data: { ... } }`
    // 单对象响应误判为过期。
    let name_present = extract_name_from_userinfo(&user_info).is_some();
    let is_session_expired = !name_present
        && (user_info["success"].as_bool() == Some(false)
            || user_info["errcode"].is_number()
            || user_info["code"].as_i64().map(|c| c != 0).unwrap_or(false)
            || user_info.get("data").is_none()
            || user_info["data"]
                .as_array()
                .map(|arr| arr.is_empty())
                .unwrap_or(false));

    if is_session_expired {
        *state.username.lock() = None;
        // 重建 client：让下一次登录尝试从干净状态开始，避免被这次过期的 jar 污染
        state.reset_client();
        log::warn!(
            "用户 {} 的 session 已过期, userinfo 响应: {}",
            username,
            user_info
        );
        return Err(AppError::SessionExpired);
    }

    // 登录验证通过后，把 jar 中**最新**的 cookies 回写磁盘（自动续期）
    let refreshed = client.dump_cookies_by_domain();
    if !refreshed.is_empty() {
        if let Err(e) = sess::save_session(&app_data_dir, &username, refreshed) {
            // 续期失败不应阻塞登录，仅警告
            log::warn!("续期保存 session 失败（不影响本次登录）: {}", e);
        } else {
            log::debug!("session 已自动续期: {}", username);
        }
    }

    Ok(user_info)
}

#[tauri::command]
pub async fn get_saved_users(app: AppHandle) -> Result<Vec<String>, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    Ok(sess::list_saved_users(&app_data_dir))
}

#[tauri::command]
pub async fn remove_saved_user(app: AppHandle, username: String) -> Result<(), AppError> {
    let app_data_dir = app_data_dir(&app)?;
    sess::remove_session(&app_data_dir, &username)
}

#[tauri::command]
pub async fn get_user_info(state: State<'_, AppState>) -> Result<Value, AppError> {
    let client = state.snapshot_client();
    client.get_user_info().await
}

/// 退出当前账号：清空内存中的用户名、cookie jar。
///
/// 不在退出路径上做续期保存——`login_with_session` 成功路径已经把最新
/// cookies 持久化了。在退出时续期反而会把可能已过期/污染的 jar 状态
/// 落盘，导致下次登录此账号一定失败（实际触发过的回归）。
#[tauri::command]
pub async fn clear_session(state: State<'_, AppState>) -> Result<(), AppError> {
    *state.username.lock() = None;
    // 重建 RainClient → 旧 cookie/jar 被 drop
    state.reset_client();
    state.cancel_all();
    log::info!("会话已清空");
    Ok(())
}

// ========== 课程 Commands ==========

#[tauri::command]
pub async fn get_course_list(state: State<'_, AppState>) -> Result<Vec<Course>, AppError> {
    let client = state.snapshot_client();
    let resp = client.get_course_list().await?;

    let list = resp["data"]["list"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|item| Course {
            classroom_id: item["classroom_id"].as_i64().unwrap_or(0),
            course_id: item["course"]["id"].as_i64().unwrap_or(0),
            course_name: item["course"]["name"].as_str().unwrap_or("").to_string(),
            teacher_name: item["teacher"]["name"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    Ok(list)
}

#[tauri::command]
pub async fn get_course_works(
    state: State<'_, AppState>,
    course_id: String,
) -> Result<Vec<Work>, AppError> {
    let client = state.snapshot_client();
    let resp = client.get_works(&course_id).await?;

    let activities = resp["data"]["activities"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|item| {
            let status_code = item["status"].as_i64().unwrap_or(-1) as i32;
            let status = WorkStatus::from_code(status_code);
            let courseware_id = item["courseware_id"].as_i64().unwrap_or(0);
            let work_type = item["type"].as_i64().unwrap_or(0) as i32;

            // type 20 的作业用 content.leaf_type_id 作为实际考试 ID
            let exam_id = if work_type == 20 {
                item["content"]["leaf_type_id"]
                    .as_i64()
                    .map(|v| v.to_string())
                    .or_else(|| {
                        item["content"]["leaf_type_id"]
                            .as_str()
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| courseware_id.to_string())
            } else {
                courseware_id.to_string()
            };

            Work {
                courseware_id,
                exam_id,
                title: item["title"].as_str().unwrap_or("").to_string(),
                status: status.label().to_string(),
                score: item["score"].as_f64(),
                problem_count: item["problem_count"].as_i64().map(|v| v as i32),
                work_type,
            }
        })
        .collect();

    Ok(activities)
}

#[tauri::command]
pub async fn get_course_ppts(
    state: State<'_, AppState>,
    course_id: String,
) -> Result<Vec<Ppt>, AppError> {
    let client = state.snapshot_client();
    let resp = client.get_ppts(&course_id).await?;

    let ppts = resp["data"]["activities"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|item| Ppt {
            courseware_id: item["courseware_id"].as_i64().unwrap_or(0),
            title: item["title"].as_str().unwrap_or("").to_string(),
            count: item["count"].as_i64().unwrap_or(0) as i32,
        })
        .collect();

    Ok(ppts)
}

// ========== 导出 Commands ==========

#[tauri::command]
pub async fn export_work_answers(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    course_name: String,
    work_id: String,
    work_name: String,
    work_type: Option<i32>,
) -> Result<String, AppError> {
    let client = state.snapshot_client();
    let wt = work_type.unwrap_or(0);

    // 1. 先初始化考试页面（获取重定向 cookie）
    if wt == 20 {
        client.init_exam_2(&course_id, &work_id).await?;
    } else {
        client.init_exam(&course_id, &work_id).await?;
    }

    // 2. 根据作业类型获取 token
    let token_resp = if wt == 20 {
        client.get_token_work_2(&course_id, &work_id).await?
    } else {
        client.get_token_work(&course_id, &work_id).await?
    };

    if token_resp["success"].as_bool() == Some(false) {
        return Err(AppError::ApiError(
            token_resp["msg"]
                .as_str()
                .unwrap_or("获取 token 失败")
                .to_string(),
        ));
    }

    // user_id 可能是数字或字符串
    let user_id = match token_resp["data"]["user_id"].as_str() {
        Some(s) => s.to_string(),
        None => token_resp["data"]["user_id"].to_string().replace('"', ""),
    };
    let token = token_resp["data"]["token"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // 3. 登录考试平台
    client.get_exam_login(&work_id, &user_id, &token).await?;

    // 4. 获取答案
    let answers = client.get_all_answer(&work_id).await?;
    if let Some(answer_data) = answers {
        let app_data_dir = app_data_dir(&app)?;
        let dir = json_store::get_answer_dir(&app_data_dir)?;
        let info = serde_json::json!({
            "exam_id": work_id,
            "exam_name": work_name,
            "exam_type": "考试试题",
            "course_name": course_name
        });
        let path = json_store::save_json(&dir, &work_id, &answer_data, &info)?;
        Ok(path)
    } else {
        Err(AppError::ApiError(
            "获取答案失败，请检查是否可以查看试卷".into(),
        ))
    }
}

/// 导出章节「测验/练习」（leaf_type=6）的答案。
///
/// 与作业/考试不同，章节测验走 MOOC 平台：
/// `get_course_sign` → `get_leaf_info`（取 leaf_type_id + sku_id）→ `get_exercise_list` → 落盘。
#[tauri::command]
pub async fn export_quiz_answers(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    course_name: String,
    leaf_id: String,
    quiz_name: String,
) -> Result<String, AppError> {
    let client = state.snapshot_client();

    // 1. 课程签名
    let sign_resp = client.get_course_sign(&course_id).await?;
    let course_sign = sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // 2. 叶子信息：取 leaf_type_id 与 sku_id（可能是数字或字符串）
    let leaf_info = client
        .get_leaf_info(&leaf_id, &course_id, &course_sign)
        .await?;
    let leaf_type_id = json_str_or_num(&leaf_info["data"]["content_info"]["leaf_type_id"]);
    let sku_id = json_str_or_num(&leaf_info["data"]["sku_id"]);
    if leaf_type_id.is_empty() || sku_id.is_empty() {
        return Err(AppError::ApiError(
            "无法获取该测验信息（缺少 leaf_type_id 或 sku_id），请确认该项为测验/练习".into(),
        ));
    }

    // 3. 拉取练习题与答案
    let resp = client
        .get_exercise_list(&course_id, &leaf_type_id, &sku_id)
        .await?;

    // 4. 保守校验响应有效（不臆测具体子字段名）
    if resp["success"].as_bool() == Some(false) || resp.get("data").map_or(true, |d| d.is_null()) {
        return Err(AppError::ApiError(
            "未获取到练习题，请确认该账号可查看该测验".into(),
        ));
    }

    // 5. 落盘到 answer 目录；加 quiz_ 前缀避免与作业文件名冲突
    let app_data_dir = app_data_dir(&app)?;
    let dir = json_store::get_answer_dir(&app_data_dir)?;
    let info = serde_json::json!({
        "exam_id": leaf_id,
        "exam_name": quiz_name,
        "exam_type": "章节测验",
        "course_name": course_name
    });
    let filename = format!("quiz_{}", leaf_id);
    let path = json_store::save_json(&dir, &filename, &resp, &info)?;
    Ok(path)
}

/// 导出课件（PPT）内嵌习题的答案。
///
/// 课件试题走 `cards/detlist` 链路，与作业 / 章节测验不同：
/// `course_id` 即 classroom_id，`courseware_id` 即 PPT 标识。
/// 落盘加 `ppt_` 前缀，与作业 / 测验文件名隔离（且不会被 `get_quiz_scores`
/// 的 `quiz_` 扫描误纳入测验得分汇总）。
#[tauri::command]
pub async fn export_ppt_answers(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    course_name: String,
    courseware_id: String,
    ppt_title: String,
) -> Result<String, AppError> {
    let client = state.snapshot_client();

    // 1. 拉取课件习题与答案（detlist）
    let resp = client
        .get_ppt_questions_answer(&course_id, &courseware_id)
        .await?;

    // 2. 保守校验：响应有效且含题目结果（不臆测全部子字段）
    let has_problems = resp["data"]["problem_results"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    if resp["success"].as_bool() == Some(false) || !has_problems {
        return Err(AppError::ApiError(
            "未获取到课件习题，请确认该课件含随堂练习且该账号可查看".into(),
        ));
    }

    // 3. 落盘到 answer 目录；加 ppt_ 前缀避免与作业 / 测验文件名冲突
    let app_data_dir = app_data_dir(&app)?;
    let dir = json_store::get_answer_dir(&app_data_dir)?;
    let info = serde_json::json!({
        "exam_id": courseware_id,
        "exam_name": ppt_title,
        "exam_type": "课件试题",
        "course_name": course_name
    });
    let filename = format!("ppt_{}", courseware_id);
    let path = json_store::save_json(&dir, &filename, &resp, &info)?;
    Ok(path)
}

#[tauri::command]
pub async fn export_exam_data(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    work_id: String,
    work_name: String,
    work_type: i32,
) -> Result<ExportResult, AppError> {
    let client = state.snapshot_client();

    // 1. 先初始化考试页面
    if work_type == 20 {
        client.init_exam_2(&course_id, &work_id).await?;
    } else {
        client.init_exam(&course_id, &work_id).await?;
    }

    // 2. 根据作业类型选择 token 获取方式
    let token_resp = if work_type == 20 {
        client.get_token_work_2(&course_id, &work_id).await?
    } else {
        client.get_token_work(&course_id, &work_id).await?
    };

    if token_resp["success"].as_bool() == Some(false) {
        return Err(AppError::ApiError(
            token_resp["msg"]
                .as_str()
                .unwrap_or("获取 token 失败")
                .to_string(),
        ));
    }

    // 3. user_id 可能是数字或字符串
    let user_id = match token_resp["data"]["user_id"].as_str() {
        Some(s) => s.to_string(),
        None => token_resp["data"]["user_id"].to_string().replace('"', ""),
    };
    let token = token_resp["data"]["token"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // 4. 登录考试平台
    client.get_exam_login(&work_id, &user_id, &token).await?;

    let app_data_dir = app_data_dir(&app)?;
    let exam_dir = json_store::get_exam_dir(&app_data_dir)?;

    // 导出答案
    let answer_path = if let Some(answer_data) = client.get_all_answer(&work_id).await? {
        let info = serde_json::json!({
            "exam_id": work_id,
            "exam_name": work_name,
            "exam_type": "考试答案"
        });
        let filename = format!("{}_answer", work_id);
        Some(json_store::save_json(
            &exam_dir,
            &filename,
            &answer_data,
            &info,
        )?)
    } else {
        None
    };

    // 导出题目
    let question_data = client.get_all_question(&work_id).await?;
    let question_path = if question_data["data"] != serde_json::json!({}) {
        let info = serde_json::json!({
            "exam_id": work_id,
            "exam_name": work_name,
            "exam_type": "考试题目"
        });
        let filename = format!("{}_question", work_id);
        Some(json_store::save_json(
            &exam_dir,
            &filename,
            &question_data,
            &info,
        )?)
    } else {
        None
    };

    Ok(ExportResult {
        answer_path,
        question_path,
        message: "导出完成".to_string(),
    })
}

#[tauri::command]
pub async fn get_answer_files(app: AppHandle) -> Result<Vec<Value>, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    Ok(json_store::list_answer_files(&app_data_dir))
}

/// 删除单个答案文件（按 list_answer_files 返回的真实 `file_name`）。
#[tauri::command]
pub async fn delete_answer_file(app: AppHandle, file_name: String) -> Result<(), AppError> {
    let app_data_dir = app_data_dir(&app)?;
    json_store::delete_answer_file(&app_data_dir, &file_name)
}

/// 批量删除答案文件：逐个删除、单个失败隔离，返回成功数与失败明细
/// `{ deleted: usize, failed: [{file_name, reason}] }`。
#[tauri::command]
pub async fn delete_answer_files(
    app: AppHandle,
    file_names: Vec<String>,
) -> Result<Value, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let mut deleted = 0usize;
    let mut failed: Vec<Value> = Vec::new();
    for name in &file_names {
        match json_store::delete_answer_file(&app_data_dir, name) {
            Ok(()) => deleted += 1,
            Err(e) => failed.push(serde_json::json!({
                "file_name": name,
                "reason": e.to_string(),
            })),
        }
    }
    Ok(serde_json::json!({ "deleted": deleted, "failed": failed }))
}

/// 把选中的答案文件打包成单个分享包，写入用户经对话框选定的 `dest_path`。
/// 返回实际打入的条目数（缺失/损坏文件会被跳过）。
#[tauri::command]
pub async fn export_answer_bundle(
    app: AppHandle,
    file_names: Vec<String>,
    dest_path: String,
) -> Result<usize, AppError> {
    if file_names.is_empty() {
        return Err(AppError::InvalidInput("未选择任何答案文件".into()));
    }
    let app_data_dir = app_data_dir(&app)?;
    json_store::export_answer_bundle(&app_data_dir, &file_names, std::path::Path::new(&dest_path))
}

/// 从用户经对话框选定的 `src_path` 分享包导入答案文件到本地 answer 目录。
/// 已存在同名文件跳过，返回 `{ imported, skipped, failed }`。
#[tauri::command]
pub async fn import_answer_bundle(app: AppHandle, src_path: String) -> Result<Value, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    json_store::import_answer_bundle(&app_data_dir, std::path::Path::new(&src_path))
}

/// 获取课程学习进度：每个 leaf 的完成度 + 整体完成度。
///
/// 返回 `{ leaf_schedules: {leaf_id: 0|1|浮点}, total_schedule: 0~1 }`。
/// 前端按 leaf_id 匹配测验/章节任务展示「完成情况」。
#[tauri::command]
pub async fn get_learn_schedule(
    state: State<'_, AppState>,
    course_id: String,
) -> Result<Value, AppError> {
    let client = state.snapshot_client();
    let sign_resp = client.get_course_sign(&course_id).await?;
    let course_sign = sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let resp = client.get_course_schedule(&course_id, &course_sign).await?;
    Ok(serde_json::json!({
        "leaf_schedules": resp["data"]["leaf_schedules"].clone(),
        "total_schedule": resp["data"]["total_schedule"].clone(),
    }))
}

/// 汇总本地已导出的章节测验得分：扫描 answer 目录全部 `quiz_*.json`。
///
/// 返回 `{ "leaf_id": {score,total,answered,count} }`；leaf_id 取文件名
/// 去 `quiz_` 前缀与 `.json` 后缀。零额外网络请求（仅读本地导出文件）。
#[tauri::command]
pub async fn get_quiz_scores(app: AppHandle) -> Result<Value, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let dir = json_store::get_answer_dir(&app_data_dir)?;
    let mut out = serde_json::Map::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let fname = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if !fname.starts_with("quiz_") || !fname.ends_with(".json") {
                continue;
            }
            let leaf_id = fname
                .trim_start_matches("quiz_")
                .trim_end_matches(".json")
                .to_string();
            if leaf_id.is_empty() {
                continue;
            }
            if let Ok(data) = json_store::load_json(&dir, &fname) {
                let (score, total, answered, count) = encode::summarize_quiz(&data);
                out.insert(
                    leaf_id,
                    serde_json::json!({
                        "score": score,
                        "total": total,
                        "answered": answered,
                        "count": count,
                    }),
                );
            }
        }
    }
    Ok(Value::Object(out))
}

/// 获取课程成绩明细（服务器权威「个人得分」），按 leaf_id 索引。
///
/// 链路：`get_course_sign` 取 `data.free_sku_id`（即 sku_id，零额外请求）→ c27 成绩接口
/// `score_detail/single/{sku_id}/0/` → 解析 `data.leaf_level_infos[]`。
/// 返回 `{ "leaf_id": {user_score, leaf_score} }`，与雨课堂后台成绩单一致
/// （测验、作业等可计分 leaf 都会出现）。前端按 leaf_id 匹配展示「得分」。
#[tauri::command]
pub async fn get_score_detail(
    state: State<'_, AppState>,
    course_id: String,
) -> Result<Value, AppError> {
    let client = state.snapshot_client();

    // 1. 课堂信息里直接带 free_sku_id（与 course_sign 同一接口，无需额外请求即得 sku_id）
    let sign_resp = client.get_course_sign(&course_id).await?;
    let sku_id = json_str_or_num(&sign_resp["data"]["free_sku_id"]);
    if sku_id.is_empty() {
        return Err(AppError::ApiError(
            "无法获取课程 sku_id，暂时无法读取成绩明细".into(),
        ));
    }

    // 2. 拉取成绩明细
    let detail = client.get_score_detail(&course_id, &sku_id).await?;

    // 3. 解析 leaf_level_infos[] → { leaf_id: {user_score, leaf_score} }
    let mut out = serde_json::Map::new();
    if let Some(arr) = detail["data"]["leaf_level_infos"].as_array() {
        for item in arr {
            let leaf_id = json_str_or_num(&item["id"]);
            if leaf_id.is_empty() {
                continue;
            }
            out.insert(
                leaf_id,
                serde_json::json!({
                    "user_score": json_f64(&item["user_score"]),
                    "leaf_score": json_f64(&item["leaf_score"]),
                }),
            );
        }
    }
    Ok(Value::Object(out))
}

// ========== 刷课 Commands ==========

#[tauri::command]
pub async fn start_auto_study(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    task_ids: Option<Vec<i64>>,
    auto_answer_quiz: Option<bool>,
    playback_rate: Option<f64>,
    concurrency: Option<usize>,
) -> Result<(), AppError> {
    if course_id.trim().is_empty() {
        return Err(AppError::InvalidInput("课程 ID 不能为空".into()));
    }
    let client = state.snapshot_client();
    let cancel = state.get_or_create_cancel(&course_id);
    let app_data_dir = app_data_dir(&app)?;
    // 倍率护栏：缺省 1.0，限制在 [1.0, 4.0]，避免异常值触发风控或停滞
    let rate = playback_rate.unwrap_or(1.0).clamp(1.0, 4.0);
    // 并行数护栏：缺省 1（串行），上限 10，防止过多并发心跳触发风控或耗尽资源
    let concurrency = concurrency.unwrap_or(1).clamp(1, 10);
    study::run_auto_study(
        app,
        client,
        course_id,
        cancel,
        task_ids,
        app_data_dir,
        auto_answer_quiz.unwrap_or(false),
        rate,
        concurrency,
    )
    .await
}

#[tauri::command]
pub async fn stop_auto_study(
    state: State<'_, AppState>,
    course_id: Option<String>,
) -> Result<(), AppError> {
    match course_id {
        Some(cid) => {
            if state.cancel_course(&cid) {
                log::info!("收到停止刷课请求: {}", cid);
            } else {
                log::warn!("尝试停止不存在的刷课任务: {}", cid);
            }
        }
        None => {
            let n = state.cancel_all();
            log::info!("收到停止全部刷课请求，影响 {} 个任务", n);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_chapter_tasks(
    state: State<'_, AppState>,
    course_id: String,
) -> Result<Vec<ChapterTask>, AppError> {
    let client = state.snapshot_client();
    let sign_resp = client.get_course_sign(&course_id).await?;
    let course_sign = sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let chapter_data = client.get_all_chapter(&course_id, &course_sign).await?;

    let raw_tasks = study::extract_tasks(&chapter_data);
    let tasks: Vec<ChapterTask> = raw_tasks
        .into_iter()
        .enumerate()
        .map(|(i, (id, name, leaf_type))| ChapterTask {
            index: i + 1,
            id,
            name,
            leaf_type,
            type_str: study::leaf_type_str(leaf_type).to_string(),
        })
        .collect();

    Ok(tasks)
}

// ========== Excel 导出 Commands ==========

#[tauri::command]
pub async fn export_excel(
    app: AppHandle,
    exam_id: String,
    exam_name: String,
) -> Result<String, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    excel::export_exam_excel(&app_data_dir, &exam_id, &exam_name)
}

#[tauri::command]
pub async fn get_exam_files(app: AppHandle) -> Result<Vec<Value>, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let exam_dir = json_store::get_exam_dir(&app_data_dir)?;
    let mut files: std::collections::HashMap<String, (bool, bool, String)> =
        std::collections::HashMap::new();

    if let Ok(entries) = std::fs::read_dir(&exam_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    let name = name.trim_end_matches(".json");
                    if let Some((base, suffix)) = name.rsplit_once('_') {
                        let entry =
                            files
                                .entry(base.to_string())
                                .or_insert((false, false, String::new()));
                        match suffix {
                            "question" => entry.0 = true,
                            "answer" => {
                                entry.1 = true;
                                // 尝试读取考试名称
                                if let Ok(content) = std::fs::read_to_string(
                                    exam_dir.join(format!("{}_answer.json", base)),
                                ) {
                                    if let Ok(data) = serde_json::from_str::<Value>(&content) {
                                        if let Some(n) = data["info"]["exam_name"].as_str() {
                                            entry.2 = n.to_string();
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let result: Vec<Value> = files
        .into_iter()
        .map(|(id, (has_q, has_a, name))| {
            serde_json::json!({
                "exam_id": id,
                "exam_name": name,
                "has_question": has_q,
                "has_answer": has_a,
                "can_export": has_q && has_a,
            })
        })
        .collect();

    Ok(result)
}

// ========== AI 自动答题 Commands ==========

/// 自动答题逐题进度事件（emit: "quiz-answer-progress"）。
#[derive(Clone, Serialize)]
pub struct QuizAnswerEvent {
    pub index: usize,
    pub total: usize,
    pub problem_id: String,
    /// preparing | running | done | failed | skipped
    pub status: String,
    /// "local"（题库）| "ai" | None
    pub source: Option<String>,
    pub is_correct: Option<bool>,
    pub message: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn emit_quiz_progress(
    app: &AppHandle,
    index: usize,
    total: usize,
    problem_id: &str,
    status: &str,
    source: Option<&str>,
    is_correct: Option<bool>,
    message: Option<String>,
) {
    let evt = QuizAnswerEvent {
        index,
        total,
        problem_id: problem_id.to_string(),
        status: status.to_string(),
        source: source.map(|s| s.to_string()),
        is_correct,
        message,
    };
    let _ = app.emit("quiz-answer-progress", evt);
}

/// 保存 AI 配置；若传入 api_key 为空且已有旧配置，则沿用旧 key（前端不回传明文）。
#[tauri::command]
pub async fn save_ai_config(app: AppHandle, config: AiConfig) -> Result<(), AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let mut cfg = config;
    if cfg.api_key.trim().is_empty() {
        if let Ok(Some(old)) = ai_config::load_ai_config(&app_data_dir) {
            cfg.api_key = old.api_key;
        }
    }
    ai_config::save_ai_config(&app_data_dir, &cfg)
}

/// 读取 AI 配置（屏蔽 api_key，仅返回是否已配置）。
#[tauri::command]
pub async fn get_ai_config(app: AppHandle) -> Result<Value, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let cfg = ai_config::load_ai_config(&app_data_dir)?.unwrap_or_default();
    Ok(serde_json::json!({
        "base_url": cfg.base_url,
        "model": cfg.model,
        "enabled": cfg.enabled,
        "has_api_key": !cfg.api_key.trim().is_empty(),
    }))
}

/// 停止指定测验的自动答题。
#[tauri::command]
pub async fn stop_quiz_auto_answer(
    state: State<'_, AppState>,
    leaf_id: String,
) -> Result<(), AppError> {
    state.cancel_course(&format!("quiz:{}", leaf_id));
    Ok(())
}

/// 章节测验自动答题：薄封装，核心逻辑见 [`quiz_runner::run_quiz_answer`]。
///
/// 逐题/准备进度映射为 `quiz-answer-progress` 事件，结束 emit `quiz-answer-complete`；
/// 被取消时补发 `quiz-answer-stopped`。取消令牌以 `quiz:{leaf_id}` 维度，
/// 避免与刷课的 `course_id` 键冲突。`dry_run=true` 只生成并展示答案、**不提交**。
#[tauri::command]
pub async fn start_quiz_auto_answer(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    leaf_id: String,
    dry_run: bool,
) -> Result<QuizAnswerResult, AppError> {
    let client = state.snapshot_client();
    let app_data_dir = app_data_dir(&app)?;

    // 取消令牌：以 quiz:{leaf_id} 维度，避免与刷课的 course_id 键冲突
    let cancel = state.get_or_create_cancel(&format!("quiz:{}", leaf_id));
    cancel.store(false, Ordering::Relaxed);

    // 把核心进度回调映射为 quiz-answer-progress 事件（preparing 阶段也照发）。
    let app_for_progress = app.clone();
    let mut on_progress = move |p: quiz_runner::QuizProgress| {
        emit_quiz_progress(
            &app_for_progress,
            p.index,
            p.total,
            &p.problem_id,
            p.status,
            p.source,
            p.is_correct,
            p.message,
        );
    };

    let result = quiz_runner::run_quiz_answer(
        &client,
        &app_data_dir,
        &course_id,
        &leaf_id,
        dry_run,
        &cancel,
        &mut on_progress,
    )
    .await;

    // 核心循环被取消时静默 break，这里补发 stopped 事件（与原行为一致）。
    if cancel.load(Ordering::Relaxed) {
        let _ = app.emit("quiz-answer-stopped", serde_json::json!({"message":"已停止"}));
    }
    let result = result?;
    let _ = app.emit("quiz-answer-complete", result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_name_array_form() {
        let resp = json!({ "data": [{ "user_id": 123, "name": "张三" }] });
        assert_eq!(extract_name_from_userinfo(&resp), Some("张三".to_string()));
    }

    #[test]
    fn extract_name_object_form() {
        let resp = json!({ "data": { "user_id": 123, "name": "李四" } });
        assert_eq!(extract_name_from_userinfo(&resp), Some("李四".to_string()));
    }

    #[test]
    fn extract_name_uppercase_field() {
        let resp = json!({ "data": [{ "UserID": 123, "Name": "王五" }] });
        assert_eq!(extract_name_from_userinfo(&resp), Some("王五".to_string()));
    }

    #[test]
    fn extract_name_empty_returns_none() {
        let resp = json!({ "data": [] });
        assert_eq!(extract_name_from_userinfo(&resp), None);
        let resp = json!({ "data": [{ "user_id": 1, "name": "" }] });
        assert_eq!(extract_name_from_userinfo(&resp), None);
        let resp = json!({});
        assert_eq!(extract_name_from_userinfo(&resp), None);
    }
}
