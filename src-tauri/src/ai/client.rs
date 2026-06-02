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

        let status = resp.status();
        let data: Value = resp.json().await?;
        if !status.is_success() {
            let msg = data["error"]["message"]
                .as_str()
                .unwrap_or("AI 接口调用失败");
            return Err(AppError::ApiError(format!("AI({}): {}", status, msg)));
        }

        data["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| AppError::ApiError("AI 返回格式异常（无 content）".into()))
    }
}

/// 按题型构造用户提示，输出格式约束与 `encode` 层解析规则严格对齐。
fn build_prompt(body: &str, options: &[(String, String)], qtype: ProblemType) -> String {
    let mut opts = String::new();
    for (label, text) in options {
        opts.push_str(&format!("{}. {}\n", label, text));
    }
    match qtype {
        ProblemType::SingleChoice => format!(
            "以下是单选题。\n题干：{}\n选项：\n{}只输出一个正确选项的字母（如 A）。",
            body, opts
        ),
        ProblemType::MultiChoice => format!(
            "以下是多选题。\n题干：{}\n选项：\n{}输出所有正确选项字母，按字母升序、用英文分号分隔（如 A;C;D）。",
            body, opts
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
