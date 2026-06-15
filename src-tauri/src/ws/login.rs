use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::error::AppError;

const WS_LOGIN_URL: &str = "wss://www.yuketang.cn/wsapp/";

/// 二维码扫描超时（180 秒）。
const QR_LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

/// 单次 WebSocket 消息等待超时（30 秒），防止服务端长时间不发包导致挂起。
const WS_RECV_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, serde::Serialize)]
pub struct QrCodeEvent {
    pub url: String,
}

#[derive(Clone, serde::Serialize)]
pub struct LoginSuccessEvent {
    pub user_id: i64,
    pub name: String,
    pub school: String,
    pub auth: String,
}

/// 启动 WebSocket 登录流程。
///
/// 总超时 180s。如果用户在此期间不扫码，返回 `WebSocket("二维码已超时...")`，
/// 调用方据此提示用户重新获取二维码，而不是一直挂起占用 Tokio 任务。
pub async fn start_qr_login(app: AppHandle) -> Result<LoginSuccessEvent, AppError> {
    timeout(QR_LOGIN_TIMEOUT, qr_login_inner(app))
        .await
        .map_err(|_| AppError::WebSocket("二维码已超时，请重新获取".to_string()))?
}

async fn qr_login_inner(app: AppHandle) -> Result<LoginSuccessEvent, AppError> {
    let (ws_stream, _) = connect_async(WS_LOGIN_URL)
        .await
        .map_err(|e| AppError::WebSocket(format!("连接失败: {}", e)))?;

    let (mut write, mut read) = ws_stream.split();

    // 请求二维码
    let request_qr = serde_json::json!({
        "op": "requestlogin",
        "role": "web",
        "version": 1.4,
        "type": "qrcode",
        "from": "web"
    });
    write
        .send(Message::Text(request_qr.to_string().into()))
        .await
        .map_err(|e| AppError::WebSocket(format!("发送消息失败: {}", e)))?;

    // 监听消息（每次最多等 WS_RECV_TIMEOUT）
    loop {
        let next = timeout(WS_RECV_TIMEOUT, read.next())
            .await
            .map_err(|_| AppError::WebSocket("等待消息超时（30s 内无响应）".to_string()))?;

        let msg = match next {
            Some(m) => m.map_err(|e| AppError::WebSocket(format!("接收消息失败: {}", e)))?,
            None => return Err(AppError::WebSocket("WebSocket 连接意外关闭".into())),
        };

        if let Message::Text(text) = msg {
            let response: Value = serde_json::from_str(&text)?;

            // 收到二维码
            if let Some(qrcode) = response.get("qrcode").and_then(|v| v.as_str()) {
                log::info!("收到二维码 URL");
                let _ = app.emit(
                    "qr-code",
                    QrCodeEvent {
                        url: qrcode.to_string(),
                    },
                );
            }

            // 收到登录成功
            if response.get("subscribe_status").and_then(|v| v.as_bool()) == Some(true) {
                let user_id = response["UserID"].as_i64().unwrap_or(0);
                let name = response["Name"].as_str().unwrap_or("").to_string();
                let school = response["School"].as_str().unwrap_or("").to_string();
                let auth = response["Auth"].as_str().unwrap_or("").to_string();

                let event = LoginSuccessEvent {
                    user_id,
                    name: name.clone(),
                    school: school.clone(),
                    auth: auth.clone(),
                };

                let _ = app.emit("login-success", event.clone());
                log::info!("登录成功: {} - {}", name, school);
                return Ok(event);
            }
        }
    }
}
