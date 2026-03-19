use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub cookies: std::collections::HashMap<String, String>,
    pub username: String,
}

/// 获取用户数据目录
pub fn get_users_dir(app_data_dir: &PathBuf) -> PathBuf {
    let dir = app_data_dir.join("users");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
    }
    dir
}

/// 保存 session 到文件
pub fn save_session(
    app_data_dir: &PathBuf,
    username: &str,
    cookies: &std::collections::HashMap<String, String>,
) -> Result<(), AppError> {
    let dir = get_users_dir(app_data_dir);
    let file_path = dir.join(format!("{}.json", username));

    let data = SessionData {
        cookies: cookies.clone(),
        username: username.to_string(),
    };

    let json = serde_json::to_string_pretty(&data)?;
    std::fs::write(&file_path, json)?;
    log::info!("Session 已保存: {}", file_path.display());
    Ok(())
}

/// 从文件加载 session
pub fn load_session(
    app_data_dir: &PathBuf,
    username: &str,
) -> Result<SessionData, AppError> {
    let dir = get_users_dir(app_data_dir);
    let file_path = dir.join(format!("{}.json", username));

    let content = std::fs::read_to_string(&file_path)?;
    let data: SessionData = serde_json::from_str(&content)?;
    log::info!("Session 已加载: {}", file_path.display());
    Ok(data)
}

/// 列出所有已保存用户
pub fn list_saved_users(app_data_dir: &PathBuf) -> Vec<String> {
    let dir = get_users_dir(app_data_dir);
    let mut users = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    users.push(name.trim_end_matches(".json").to_string());
                }
            }
        }
    }

    users
}

/// 删除已保存用户的 session 文件
pub fn remove_session(app_data_dir: &PathBuf, username: &str) -> Result<(), AppError> {
    let dir = get_users_dir(app_data_dir);
    let file_path = dir.join(format!("{}.json", username));

    if file_path.exists() {
        std::fs::remove_file(&file_path)?;
        log::info!("Session 已删除: {}", file_path.display());
    } else {
        log::info!("Session 文件不存在，视为已删除: {}", file_path.display());
    }

    Ok(())
}
