//! AI 自动答题相关模块。
//!
//! - `config`：AI 模型配置（OpenAI 兼容）的加密存取。
//! - `client`：OpenAI 兼容大模型调用（客观题兜底）。
//! - `encode`：章节测验题目解析、题库匹配与 AI 答案编码。

pub mod client;
pub mod config;
pub mod encode;
