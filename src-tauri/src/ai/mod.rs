//! AI 自动答题相关模块。
//!
//! - `config`：AI 模型配置（OpenAI 兼容）的加密存取。
//! - `client`：OpenAI 兼容大模型调用（客观题兜底）。
//! - `encode`：章节测验题目解析、题库匹配与 AI 答案编码。
//! - `font_decode`：运行时下载并动态解混淆雨课堂加密字体。
//! - `quiz_runner`：自动答题共享核心（单测验/批量/刷课复用）。

pub mod client;
pub mod config;
pub mod encode;
pub mod font_decode;
pub mod quiz_runner;
