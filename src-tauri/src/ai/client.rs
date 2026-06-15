//! OpenAI 兼容大模型客户端（仅用于客观题兜底）。
//!
//! 用独立的 `reqwest::Client`（不挂雨课堂 cookie jar），POST 标准
//! `/v1/chat/completions`。Prompt 严格约束模型只输出可解析的最小答案，
//! 输出格式与 `encode::encode_ai_answer` 的解析规则一一对应。

use serde_json::Value;

use crate::ai::config::AiConfig;
use crate::ai::encode::ProblemType;
use crate::error::AppError;

/// 系统提示：强约束模型只输出最终答案，禁止任何解释/推理/多余标点。
const SYSTEM_PROMPT: &str =
    "你是答题助手。只输出最终答案本身，禁止任何解释、推理过程、前后缀或多余标点。严格遵守用户给定的输出格式。";

pub struct AiClient {
    http: reqwest::Client,
    cfg: AiConfig,
}

impl AiClient {
    pub fn new(cfg: AiConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            cfg,
        }
    }

    /// 求解单道客观题，返回模型原始文本输出（交由 `encode` 层解析）。
    pub async fn solve_objective(
        &self,
        question_body: &str,
        options: &[(String, String)],
        qtype: ProblemType,
    ) -> Result<String, AppError> {
        let url = format!(
            "{}/v1/chat/completions",
            self.cfg.base_url.trim_end_matches('/')
        );
        let user_prompt = build_prompt(question_body, options, qtype);

        let body = serde_json::json!({
            "model": self.cfg.model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0,
            "stream": false
        });

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.cfg.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        // 先取状态码与原始响应体：非 2xx 且响应体不是 JSON（如 HTML 错误页 / 网关纯文本）时，
        // 直接 resp.json() 会因解析失败而吞掉状态码，故先拿文本再容错解析。
        let status = resp.status();
        let code = status.as_u16();
        let text = resp.text().await?;
        let data: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

        // 1) HTTP 状态码非 2xx：稳健报错（兼容非 JSON 体）+ 按类型给排查提示。
        if !status.is_success() {
            let msg = extract_error_message(&data).unwrap_or_else(|| body_snippet(&text));
            return Err(AppError::ApiError(format!(
                "AI 接口失败 {}{}: {}",
                code,
                status_hint(code),
                msg
            )));
        }

        // 2) 部分供应商出错仍返回 HTTP 200：响应体非 JSON，或携带 error 对象 → 一律判失败。
        if data.is_null() {
            return Err(AppError::ApiError(format!(
                "AI 返回非 JSON 响应（HTTP {}）: {}",
                code,
                body_snippet(&text)
            )));
        }
        if let Some(msg) = extract_error_message(&data) {
            return Err(AppError::ApiError(format!(
                "AI 返回错误（HTTP {}）: {}",
                code, msg
            )));
        }

        // 3) 提取并校验 content：trim 后为空也视为失败（绝不把空答案交给上层）。
        data["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::ApiError("AI 返回格式异常（无 content 或为空）".into()))
    }
}

/// 从响应体提取错误信息：兼容 `error` 为对象（取其 `message`）或直接为字符串。
fn extract_error_message(data: &Value) -> Option<String> {
    match data.get("error") {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(Value::Object(o)) => o
            .get("message")
            .and_then(|m| m.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    }
}

/// HTTP 状态码 → 排查提示（区分鉴权 / 限流 / 服务端等常见错误）。
fn status_hint(code: u16) -> &'static str {
    match code {
        400 => "（请求无效，请检查模型名称/参数）",
        401 => "（鉴权失败，请检查 API Key）",
        403 => "（无权限，请检查 API Key 或模型访问权限）",
        404 => "（接口地址不存在，请检查 Base URL）",
        429 => "（请求过于频繁或额度不足）",
        500..=599 => "（AI 服务端错误，请稍后重试）",
        _ => "",
    }
}

/// 截取响应体片段用于报错（按字符截断，避免超长 / 噪声刷屏）。
fn body_snippet(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "（空响应体）".to_string();
    }
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i >= 200 {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

/// 按题型构造用户提示，输出格式约束与 `encode` 层解析规则严格对齐。
fn build_prompt(body: &str, options: &[(String, String)], qtype: ProblemType) -> String {
    let mut opts = String::new();
    for (label, text) in options {
        opts.push_str(&format!("{}. {}\n", label, text));
    }
    let labels = options
        .iter()
        .map(|(label, _)| label.trim())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    match qtype {
        ProblemType::SingleChoice => format!(
            "以下是单选题。\n题干：{}\n选项：\n{}合法选项标签：{}。\n只输出一个正确选项标签（如 A）。禁止输出 Answer:、解释或其它文字。",
            body, opts, labels
        ),
        ProblemType::MultiChoice => format!(
            "以下是多选题。\n题干：{}\n选项：\n{}合法选项标签：{}。\n只输出所有正确选项标签，按标签升序、用英文分号分隔（如 A;C;D）。禁止输出 Answer:、解释或其它文字。",
            body, opts, labels
        ),
        ProblemType::Judge => format!(
            "以下是判断题。\n题干：{}\n若说法正确请只输出「正确」，错误请只输出「错误」。不要输出其它内容。",
            body
        ),
        ProblemType::FillBlank => format!(
            "以下是填空题。\n题干：{}\n逐空作答，每行一个，格式严格为「序号=答案」（如 1=北京）。不要输出其它内容。",
            body
        ),
    }
}
