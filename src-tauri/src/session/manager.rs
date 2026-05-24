use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::util::{crypto, sanitize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub cookies: std::collections::HashMap<String, String>,
    pub username: String,
}

/// 加密 session 文件扩展名（与旧的明文 .json 区分，便于迁移）。
const SESSION_EXT: &str = "session";

/// 获取用户数据目录（按需创建）。
pub fn get_users_dir(app_data_dir: &Path) -> Result<PathBuf, AppError> {
    let dir = app_data_dir.join("users");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// 在 users 目录下构造**经过清洗的**会话文件路径。
fn session_file_path(app_data_dir: &Path, username: &str) -> Result<PathBuf, AppError> {
    let dir = get_users_dir(app_data_dir)?;
    let clean = sanitize::sanitize_filename(username)?;
    let path = dir.join(format!("{}.{}", clean, SESSION_EXT));
    // 二次校验：清洗后路径必须仍在 users 目录内
    if !path.starts_with(&dir) {
        return Err(AppError::InvalidInput(format!(
            "非法用户名: {}",
            username
        )));
    }
    Ok(path)
}

/// 旧明文 .json 文件路径（仅用于迁移）。
fn legacy_session_path(app_data_dir: &Path, username: &str) -> Result<PathBuf, AppError> {
    let dir = get_users_dir(app_data_dir)?;
    let clean = sanitize::sanitize_filename(username)?;
    Ok(dir.join(format!("{}.json", clean)))
}

/// 保存 session（加密落盘）。
pub fn save_session(
    app_data_dir: &Path,
    username: &str,
    cookies: &std::collections::HashMap<String, String>,
) -> Result<(), AppError> {
    let file_path = session_file_path(app_data_dir, username)?;

    let data = SessionData {
        cookies: cookies.clone(),
        username: username.to_string(),
    };
    let plaintext = serde_json::to_vec(&data)?;
    let encrypted = crypto::encrypt(app_data_dir, &plaintext)?;
    std::fs::write(&file_path, encrypted)?;

    // Unix 下进一步限制为 0600（owner only）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o600));
    }

    log::info!("Session 已加密保存: {}", file_path.display());
    Ok(())
}

/// 从文件加载 session，自动迁移旧明文 .json。
pub fn load_session(app_data_dir: &Path, username: &str) -> Result<SessionData, AppError> {
    let file_path = session_file_path(app_data_dir, username)?;

    // 加密文件优先
    if file_path.exists() {
        let container = std::fs::read_to_string(&file_path)?;
        let plaintext = crypto::decrypt(app_data_dir, &container)?;
        let data: SessionData = serde_json::from_slice(&plaintext)?;
        return Ok(data);
    }

    // 兼容旧明文 session（自动加密迁移后删除）
    let legacy = legacy_session_path(app_data_dir, username)?;
    if legacy.exists() {
        log::warn!(
            "检测到旧明文 session 文件，将自动加密迁移: {}",
            legacy.display()
        );
        let content = std::fs::read_to_string(&legacy)?;
        let data: SessionData = serde_json::from_str(&content)?;
        save_session(app_data_dir, username, &data.cookies)?;
        let _ = std::fs::remove_file(&legacy);
        return Ok(data);
    }

    Err(AppError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("会话文件不存在: {}", username),
    )))
}

/// 列出所有已保存用户（同时识别新 .session 和旧 .json）。
pub fn list_saved_users(app_data_dir: &Path) -> Vec<String> {
    let dir = match get_users_dir(app_data_dir) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut users = std::collections::BTreeSet::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(&format!(".{}", SESSION_EXT)) {
                    users.insert(stem.to_string());
                } else if let Some(stem) = name.strip_suffix(".json") {
                    users.insert(stem.to_string());
                }
            }
        }
    }

    users.into_iter().collect()
}

/// 删除指定用户的所有会话文件（新加密文件 + 旧明文文件）。
pub fn remove_session(app_data_dir: &Path, username: &str) -> Result<(), AppError> {
    let encrypted_path = session_file_path(app_data_dir, username)?;
    let legacy_path = legacy_session_path(app_data_dir, username)?;

    let mut removed_any = false;
    for path in [&encrypted_path, &legacy_path] {
        if path.exists() {
            std::fs::remove_file(path)?;
            log::info!("Session 已删除: {}", path.display());
            removed_any = true;
        }
    }
    if !removed_any {
        log::info!("Session 文件不存在，视为已删除: {}", username);
    }
    Ok(())
}
