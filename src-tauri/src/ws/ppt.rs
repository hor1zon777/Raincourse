use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::error::AppError;

const WS_PPT_URL: &str = "wss://www.yuketang.cn/ws/";

/// 发送 PPT 浏览记录
pub async fn view_ppt(class_id: &str, user_id: i64, page_count: i32) -> Result<(), AppError> {
    let (ws_stream, _) = connect_async(WS_PPT_URL)
        .await
        .map_err(|e| AppError::WebSocket(format!("PPT WS 连接失败: {}", e)))?;

    let (mut write, mut read) = ws_stream.split();

    // 发送浏览记录
    let timestamp = chrono::Utc::now().timestamp();
    let view_record = serde_json::json!({
        "op": "view_record",
        "cardsID": class_id,
        "type": "cache",
        "data": vec![1i32; page_count as usize],
        "start_time": timestamp,
        "platform": "web",
        "user_id": user_id,
    });

    write
        .send(Message::Text(view_record.to_string().into()))
        .await
        .map_err(|e| AppError::WebSocket(format!("发送浏览记录失败: {}", e)))?;

    // 发送 view_record_answer
    let view_answer = serde_json::json!({
        "op": "view_record_answer",
        "cardsID": class_id,
        "type": "page",
        "platform": "web",
    });

    write
        .send(Message::Text(view_answer.to_string().into()))
        .await
        .map_err(|e| AppError::WebSocket(format!("发送 answer 记录失败: {}", e)))?;

    // 等待响应
    if let Some(Ok(Message::Text(text))) = read.next().await {
        let response: Value = serde_json::from_str(&text).unwrap_or_default();
        if response.get("errmsg").and_then(|v| v.as_str()) == Some("正确") {
            log::info!("PPT 浏览完成");
        }
    }

    Ok(())
}
