//! 章节测验/练习自动答题的共享核心。
//!
//! 被三处复用：单测验命令 [`crate::commands::start_quiz_auto_answer`]、前端批量答题、
//! 以及刷课流程 [`crate::study::run_auto_study`]（开启「刷课时自动答测验」开关后）。
//!
//! 通过 [`QuizProgress`] 进度回调与具体事件名解耦：调用方自行把进度映射成
//! `quiz-answer-progress`（单测验/批量）或 `study-task-update`（刷课）事件。
//!
//! 安全原则沿用 [`crate::ai::encode`]：任何字段提取 / 编码失败都「跳过该题、不提交」，
//! 加密字体未能解码的题绝不发给 AI；已提交过的题（`encode::is_answered`）直接跳过。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::time::sleep;

use crate::ai::client::AiClient;
use crate::ai::config as ai_config;
use crate::ai::encode::{self, ProblemType};
use crate::ai::font_decode;
use crate::api::client::RainClient;
use crate::error::AppError;
use crate::storage::json_store;
use crate::util::json_str_or_num;

/// 自动答题汇总结果（单测验命令返回 + emit: "quiz-answer-complete"）。
#[derive(Clone, Serialize)]
pub struct QuizAnswerResult {
    pub total: usize,
    pub submitted: usize,
    pub correct: usize,
    pub from_local: usize,
    pub from_ai: usize,
    pub failed: usize,
    pub skipped: usize,
    /// 已提交过而被跳过的题数（不重复作答）。
    pub already_answered: usize,
}

impl QuizAnswerResult {
    fn new(total: usize) -> Self {
        Self {
            total,
            submitted: 0,
            correct: 0,
            from_local: 0,
            from_ai: 0,
            failed: 0,
            skipped: 0,
            already_answered: 0,
        }
    }
}

/// 逐题/准备阶段的进度单元。`status` 取 `preparing | running | done | failed | skipped`，
/// 准备阶段用 `index = 0`、`problem_id = ""`。
pub struct QuizProgress {
    pub index: usize,
    pub total: usize,
    pub problem_id: String,
    pub status: &'static str,
    /// "local"（题库）| "ai" | None
    pub source: Option<&'static str>,
    pub is_correct: Option<bool>,
    pub message: Option<String>,
}

/// 准备阶段进度（题数未知时 total 传 0）。
fn prep(on: &mut (dyn FnMut(QuizProgress) + Send), total: usize, msg: impl Into<String>) {
    on(QuizProgress {
        index: 0,
        total,
        problem_id: String::new(),
        status: "preparing",
        source: None,
        is_correct: None,
        message: Some(msg.into()),
    });
}

#[allow(clippy::too_many_arguments)]
fn step(
    on: &mut (dyn FnMut(QuizProgress) + Send),
    index: usize,
    total: usize,
    problem_id: &str,
    status: &'static str,
    source: Option<&'static str>,
    is_correct: Option<bool>,
    message: Option<String>,
) {
    on(QuizProgress {
        index,
        total,
        problem_id: problem_id.to_string(),
        status,
        source,
        is_correct,
        message,
    });
}

fn count_answer_source(result: &mut QuizAnswerResult, source: Option<&str>) {
    match source {
        Some("local") => result.from_local += 1,
        Some("ai") => result.from_ai += 1,
        _ => {}
    }
}

fn truncate_progress_text(s: &str) -> String {
    const MAX_CHARS: usize = 120;
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= MAX_CHARS {
            out.push('…');
            return out;
        }
        out.push(c);
    }
    out
}

fn exercise_font_url(exercise: &Value) -> Option<String> {
    exercise["data"]["font"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn format_unknown_chars(chars: &[char]) -> String {
    let mut s = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i >= 20 {
            s.push('…');
            break;
        }
        s.push(*c);
    }
    s
}

/// 从 problem_apply 响应解析是否答对；解析不出返回 None。
///
/// 同平台读取接口用 `is_right` 标记正误，提交响应大概率同名，故多路径容错：
/// `data.is_correct` / `data.is_right` / `data.correct` 及对应顶层字段。
fn parse_is_correct(resp: &Value) -> Option<bool> {
    const PATHS: &[&[&str]] = &[
        &["data", "is_correct"],
        &["data", "is_right"],
        &["data", "correct"],
        &["is_correct"],
        &["is_right"],
        &["correct"],
    ];
    for path in PATHS {
        let mut cur = resp;
        let mut ok = true;
        for seg in *path {
            match cur.get(*seg) {
                Some(n) => cur = n,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            if let Some(b) = cur.as_bool() {
                return Some(b);
            }
        }
    }
    None
}

/// 提交响应是否被服务端业务层拒绝（`success` 显式为 `false`）。
///
/// 沿用全局惯例（拉题 `exercise["success"] == Some(false)`、commands 各处同写法）：
/// 只认**显式** `success == false`；成功响应可能不带该字段或为 `true`，均不算拒绝。
/// 这是「自动答题漏掉提交」的关键防线——此前提交只要 HTTP 成功就计入已提交，
/// 服务端因风控限流 / CSRF 失效 / 答案格式不符返回的 `success:false` 被静默当成功，
/// 答案实际未入库，用户却看到「已提交」，表现为「有时候漏掉提交」。
fn is_submit_rejected(resp: &Value) -> bool {
    resp["success"].as_bool() == Some(false)
}

/// 从被拒绝的提交响应里容错提取可读错误信息（供进度展示与日志）。
fn submit_error_detail(resp: &Value) -> String {
    const MSG_KEYS: &[&str] = &["detail", "errmsg", "msg", "message", "error"];
    let text = MSG_KEYS.iter().find_map(|k| {
        resp.get(*k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    });
    let code = resp.get("errcode").filter(|c| !c.is_null());
    match (text, code) {
        (Some(t), Some(c)) => format!("服务端拒绝（{}，errcode={}）", t, c),
        (Some(t), None) => format!("服务端拒绝（{}）", t),
        (None, Some(c)) => format!("服务端拒绝（errcode={}）", c),
        (None, None) => format!(
            "服务端拒绝（success=false）：{}",
            truncate_progress_text(&resp.to_string())
        ),
    }
}

/// 提交单题答案，把「服务端业务层拒绝（success=false）」与网络错误**统一**成 `Err`，
/// 供上层做有限重试 / 计失败。成功时返回响应体（用于解析是否答对）。
async fn submit_answer(
    client: &RainClient,
    course_id: &str,
    problem_id: &str,
    answer: &Value,
) -> Result<Value, AppError> {
    let resp = client
        .post_exercise_answer(course_id, problem_id, answer)
        .await?;
    if is_submit_rejected(&resp) {
        return Err(AppError::ApiError(submit_error_detail(&resp)));
    }
    Ok(resp)
}

/// 章节测验自动答题核心：拉题 → 已答跳过 → 题库优先 / AI 兜底 →（非 dry_run 则）逐题提交。
///
/// 全程串行，逐题回调进度；限速 ≥1 题/秒；单题失败/跳过隔离，不中断整体；
/// 通过 `cancel` 支持取消（**不**在此重置，由调用方负责）。
/// `dry_run=true` 只生成并展示答案、**不提交**。
pub async fn run_quiz_answer(
    client: &RainClient,
    app_data_dir: &Path,
    course_id: &str,
    leaf_id: &str,
    dry_run: bool,
    cancel: &Arc<AtomicBool>,
    on_progress: &mut (dyn FnMut(QuizProgress) + Send),
) -> Result<QuizAnswerResult, AppError> {
    // 1. 课程签名 + 叶子信息
    prep(on_progress, 0, "获取测验信息…");
    let sign_resp = client.get_course_sign(course_id).await?;
    let course_sign = sign_resp["data"]["course_sign"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let leaf_info = client
        .get_leaf_info(leaf_id, course_id, &course_sign)
        .await?;
    let leaf_type_id = json_str_or_num(&leaf_info["data"]["content_info"]["leaf_type_id"]);
    let sku_id = json_str_or_num(&leaf_info["data"]["sku_id"]);
    if leaf_type_id.is_empty() || sku_id.is_empty() {
        return Err(AppError::ApiError(
            "无法获取该测验信息（缺少 leaf_type_id 或 sku_id），请确认该项为测验/练习".into(),
        ));
    }

    // 2. 拉取现场题目列表
    prep(on_progress, 0, "拉取题目列表…");
    let exercise = client
        .get_exercise_list(course_id, &leaf_type_id, &sku_id)
        .await?;
    if exercise["success"].as_bool() == Some(false)
        || exercise.get("data").map_or(true, |d| d.is_null())
    {
        return Err(AppError::ApiError(
            "未获取到练习题，请确认该账号可查看该测验".into(),
        ));
    }
    let problems: Vec<Value> = encode::find_problems(&exercise)
        .map(|v| v.into_iter().cloned().collect())
        .unwrap_or_default();
    let exercise_font_url = exercise_font_url(&exercise);
    let total = problems.len();

    // 3. 加载本地题库（可不存在）
    let answer_dir = json_store::get_answer_dir(app_data_dir)?;
    let local_bank: Option<Value> =
        json_store::load_json(&answer_dir, &format!("quiz_{}", leaf_id)).ok();

    // 4. AI 配置（仅启用且齐全时构造 client）
    let ai_client = ai_config::load_ai_config(app_data_dir)
        .ok()
        .flatten()
        .filter(|c| c.is_usable())
        .map(AiClient::new);

    // 加密字体运行时解码表：仅当启用 AI 兜底且本测验返回了加密字体时构建
    //（下载字体 + 与内嵌思源黑体逐字比对，整测验只建一次、全题复用）。
    let font_decoder = if ai_client.is_some() {
        match exercise_font_url.as_deref() {
            Some(url) => {
                prep(on_progress, total, "构建加密字体解码表（下载并逐字比对）…");
                match font_decode::build_decode_map(url).await {
                    Ok(m) => {
                        log::info!(
                            "测验 {} 加密字体解码表构建成功，覆盖 {} 字",
                            leaf_id,
                            m.len()
                        );
                        prep(on_progress, total, format!("解码表就绪，覆盖 {} 字", m.len()));
                        Some(m)
                    }
                    Err(e) => {
                        log::warn!("测验 {} 加密字体解码表构建失败: {}", leaf_id, e);
                        prep(on_progress, total, "加密字体解码表构建失败，相关题将跳过");
                        None
                    }
                }
            }
            None => None,
        }
    } else {
        None
    };

    let mut result = QuizAnswerResult::new(total);

    for (i, p) in problems.iter().enumerate() {
        // 限速 + 取消检查（除第一题外，每题前 sleep 1100ms ≈ ≤1 题/秒）
        if i > 0 {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            sleep(Duration::from_millis(1100)).await;
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let idx = i + 1;
        let problem_id = match encode::extract_problem_id(p) {
            Some(id) => id,
            None => {
                result.skipped += 1;
                step(
                    on_progress,
                    idx,
                    total,
                    "",
                    "skipped",
                    None,
                    None,
                    Some("无法识别题目 ID".into()),
                );
                continue;
            }
        };

        // 已提交过的题直接跳过（凡提交过即跳过，不重复作答）。
        if encode::is_answered(p) {
            result.already_answered += 1;
            step(
                on_progress,
                idx,
                total,
                &problem_id,
                "skipped",
                None,
                None,
                Some("已作答，跳过".into()),
            );
            continue;
        }

        step(on_progress, idx, total, &problem_id, "running", None, None, None);

        // 答案来源：题库优先
        let mut answer: Option<Value> = None;
        let mut source: Option<&'static str> = None;
        let mut ai_raw: Option<String> = None;
        if let Some(bank) = &local_bank {
            if let Some(a) = encode::lookup_local(&problem_id, bank) {
                answer = Some(a);
                source = Some("local");
                step(
                    on_progress,
                    idx,
                    total,
                    &problem_id,
                    "running",
                    Some("local"),
                    None,
                    Some("本地题库命中".into()),
                );
            }
        }
        // AI 兜底
        if answer.is_none() {
            if let Some(ai) = &ai_client {
                match ProblemType::from_question(p) {
                    Some(qtype) => {
                        let body = encode::extract_body_with_decoder(p, font_decoder.as_ref());
                        let options =
                            encode::extract_options_with_decoder(p, font_decoder.as_ref());
                        // 加密字体未能解码 → 跳过该题，绝不把乱码/可能解错的题干发给 AI。
                        let unknown_chars = encode::encrypted_font_unknown_chars_with_decoder(
                            p,
                            font_decoder.as_ref(),
                        );
                        if !unknown_chars.is_empty() {
                            result.skipped += 1;
                            let detail = if font_decoder.is_none() {
                                format!(
                                    "加密字体未能下载/解析，已跳过 AI；font={}",
                                    exercise_font_url.as_deref().unwrap_or("未返回")
                                )
                            } else {
                                format!(
                                    "加密字体仍有未解码字符「{}」，已跳过 AI",
                                    format_unknown_chars(&unknown_chars)
                                )
                            };
                            step(
                                on_progress,
                                idx,
                                total,
                                &problem_id,
                                "skipped",
                                Some("ai"),
                                None,
                                Some(detail),
                            );
                            continue;
                        }
                        if matches!(qtype, ProblemType::SingleChoice | ProblemType::MultiChoice)
                            && options.is_empty()
                        {
                            result.skipped += 1;
                            step(
                                on_progress,
                                idx,
                                total,
                                &problem_id,
                                "skipped",
                                Some("ai"),
                                None,
                                Some("选择题缺少选项，已跳过 AI 兜底以避免猜测".into()),
                            );
                            continue;
                        }
                        step(
                            on_progress,
                            idx,
                            total,
                            &problem_id,
                            "running",
                            Some("ai"),
                            None,
                            Some(format!(
                                "题库未命中，正在调用 AI 兜底；题干: {}",
                                truncate_progress_text(&body)
                            )),
                        );
                        log::info!(
                            "测验 {} 题 {} 题库未命中，开始调用 AI 兜底",
                            leaf_id,
                            problem_id
                        );
                        match ai.solve_objective(&body, &options, qtype).await {
                            Ok(raw) => {
                                match encode::encode_ai_answer_with_options(&raw, qtype, &options) {
                                    Ok(enc) => {
                                        ai_raw = Some(raw);
                                        answer = Some(enc);
                                        source = Some("ai");
                                    }
                                    Err(e) => {
                                        result.skipped += 1;
                                        step(
                                            on_progress,
                                            idx,
                                            total,
                                            &problem_id,
                                            "skipped",
                                            Some("ai"),
                                            None,
                                            Some(format!("AI 答案解析失败: {}", e)),
                                        );
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                result.failed += 1;
                                step(
                                    on_progress,
                                    idx,
                                    total,
                                    &problem_id,
                                    "failed",
                                    Some("ai"),
                                    None,
                                    Some(format!("AI 调用失败: {}", e)),
                                );
                                continue;
                            }
                        }
                    }
                    None => {
                        result.skipped += 1;
                        step(
                            on_progress,
                            idx,
                            total,
                            &problem_id,
                            "skipped",
                            None,
                            None,
                            Some("无法识别题型".into()),
                        );
                        continue;
                    }
                }
            }
        }

        let answer = match answer {
            Some(a) => a,
            None => {
                result.skipped += 1;
                let reason = if ai_client.is_some() {
                    "题库未命中且 AI 未能作答"
                } else {
                    "题库未命中且未启用 AI"
                };
                step(
                    on_progress,
                    idx,
                    total,
                    &problem_id,
                    "skipped",
                    None,
                    None,
                    Some(reason.into()),
                );
                continue;
            }
        };

        // dry_run：只展示将提交的内容，不真正提交
        if dry_run {
            count_answer_source(&mut result, source);
            let msg = match (source, ai_raw.as_ref()) {
                (Some("ai"), Some(raw)) => format!(
                    "[试跑] AI 原始输出: {}；解析后将提交: {}",
                    truncate_progress_text(raw),
                    answer
                ),
                _ => format!("[试跑] 将提交: {}", answer),
            };
            step(
                on_progress,
                idx,
                total,
                &problem_id,
                "done",
                source,
                None,
                Some(msg),
            );
            continue;
        }

        // 提交（含有限重试）：偶发限流 / 网络抖动 / CSRF 抖动下单次提交可能被拒
        //（success=false）或网络失败，是「漏掉提交」的偶发来源。提交为幂等覆盖（同题再交
        // 只更新答案、不重复计分，且循环开头已用 is_answered 跳过跨次已答题），故失败后
        // 退避重试可救回大部分偶发漏提交；重试尊重 cancel，取消后不再重试。
        const SUBMIT_MAX_ATTEMPTS: u32 = 2;
        let mut attempt = 0u32;
        let submit_result = loop {
            attempt += 1;
            match submit_answer(client, course_id, &problem_id, &answer).await {
                Ok(resp) => break Ok(resp),
                Err(e) => {
                    if attempt >= SUBMIT_MAX_ATTEMPTS || cancel.load(Ordering::Relaxed) {
                        break Err(e);
                    }
                    step(
                        on_progress,
                        idx,
                        total,
                        &problem_id,
                        "running",
                        source,
                        None,
                        Some(format!(
                            "提交未成功（{}），正在重试（第 {}/{} 次）…",
                            e,
                            attempt + 1,
                            SUBMIT_MAX_ATTEMPTS
                        )),
                    );
                    sleep(Duration::from_millis(1200)).await;
                }
            }
        };

        match submit_result {
            Ok(resp) => {
                result.submitted += 1;
                count_answer_source(&mut result, source);
                let is_correct = parse_is_correct(&resp);
                if is_correct == Some(true) {
                    result.correct += 1;
                }
                let msg = match (source, ai_raw.as_ref()) {
                    (Some("ai"), Some(raw)) => Some(format!(
                        "AI 原始输出: {}；解析后已提交: {}",
                        truncate_progress_text(raw),
                        answer
                    )),
                    _ => None,
                };
                step(
                    on_progress,
                    idx,
                    total,
                    &problem_id,
                    "done",
                    source,
                    is_correct,
                    msg,
                );
            }
            Err(e) => {
                result.failed += 1;
                step(
                    on_progress,
                    idx,
                    total,
                    &problem_id,
                    "failed",
                    source,
                    None,
                    Some(format!("提交失败: {}", e)),
                );
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_submit_rejected_only_on_explicit_false() {
        // 成功：success=true 或缺字段都不算拒绝（避免把正常提交误判为失败）
        assert!(!is_submit_rejected(&json!({"success": true})));
        assert!(!is_submit_rejected(&json!({"data": {"is_correct": true}})));
        assert!(!is_submit_rejected(&json!({})));
        // 拒绝：success 显式 false（风控限流 / CSRF 失效 / 格式不符）
        assert!(is_submit_rejected(
            &json!({"success": false, "detail": "操作过于频繁"})
        ));
    }

    #[test]
    fn submit_error_detail_extracts_message_and_code() {
        // 文案 + errcode 同时存在
        assert_eq!(
            submit_error_detail(&json!({"success": false, "detail": "操作频繁", "errcode": 4031})),
            "服务端拒绝（操作频繁，errcode=4031）"
        );
        // 仅文案（errmsg 兜底字段）
        assert_eq!(
            submit_error_detail(&json!({"success": false, "errmsg": "csrf 失效"})),
            "服务端拒绝（csrf 失效）"
        );
        // 仅 errcode
        assert_eq!(
            submit_error_detail(&json!({"success": false, "errcode": 500})),
            "服务端拒绝（errcode=500）"
        );
        // 无任何信息字段：兜底带原始响应片段
        let s = submit_error_detail(&json!({"success": false}));
        assert!(s.starts_with("服务端拒绝（success=false）："));
    }
}
