use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("网络请求失败: {0}")]
    Request(#[from] reqwest::Error),

    #[error("WebSocket 错误: {0}")]
    WebSocket(String),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("未登录")]
    NotLoggedIn,

    #[error("会话已过期，请重新扫码登录")]
    SessionExpired,

    #[error("API 返回错误: {0}")]
    ApiError(String),

    #[error("操作已被用户取消")]
    Cancelled,

    #[error("非法输入: {0}")]
    InvalidInput(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("{0}")]
    General(String),
}

impl AppError {
    /// 返回稳定的机器可读错误码，前端按此判断 UI 行为。
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Request(_) => "REQUEST_FAILED",
            AppError::WebSocket(_) => "WEBSOCKET_ERROR",
            AppError::Json(_) => "JSON_PARSE_ERROR",
            AppError::Io(_) => "IO_ERROR",
            AppError::NotLoggedIn => "NOT_LOGGED_IN",
            AppError::SessionExpired => "SESSION_EXPIRED",
            AppError::ApiError(_) => "API_ERROR",
            AppError::Cancelled => "CANCELLED",
            AppError::InvalidInput(_) => "INVALID_INPUT",
            AppError::Config(_) => "CONFIG_ERROR",
            AppError::General(_) => "GENERAL_ERROR",
        }
    }
}

/// 前端拿到的统一错误结构：`{ code: "SESSION_EXPIRED", message: "..." }`
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("code", self.code())?;
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
}
