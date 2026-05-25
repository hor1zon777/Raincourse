use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::util::{crypto, sanitize};

/// 已保存的会话数据。
///
/// 字段说明：
/// - `cookies`：**旧字段（向下兼容）**。仅 `www.yuketang.cn` 域。新代码写入时
///   会同步写入一份方便降级阅读；新代码读取时优先使用 `cookies_by_domain`。
/// - `cookies_by_domain`：**新字段**。按 host 分组的 cookies，覆盖
///   yuketang/xuetangx 多域，解决"重启后考试接口缺 cookie 而失效"的问题。
/// - `version`：格式版本号；当前 2。0/缺失=旧明文格式，1=单域加密，2=多域加密。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub username: String,
    #[serde(default)]
    pub cookies: HashMap<String, String>,
    #[serde(default)]
    pub cookies_by_domain: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub version: u8,
}

impl SessionData {
    /// 取得多域 cookies；若 session 是旧版（无 cookies_by_domain），自动从
    /// 旧 `cookies` 字段构造一个单域 fallback（www.yuketang.cn）。
    pub fn effective_cookies_by_domain(&self) -> HashMap<String, HashMap<String, String>> {
        if !self.cookies_by_domain.is_empty() {
            return self.cookies_by_domain.clone();
        }
        let mut fallback = HashMap::new();
        if !self.cookies.is_empty() {
            fallback.insert("www.yuketang.cn".to_string(), self.cookies.clone());
        }
        fallback
    }
}

/// 加密 session 文件扩展名（与旧的明文 .json 区分，便于迁移）。
const SESSION_EXT: &str = "session";

/// 当前 session 格式版本（多域 + 加密）。
const SESSION_VERSION: u8 = 2;

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
///
/// 参数 `cookies_by_domain` 形如：
/// `{ "www.yuketang.cn": { "sessionid": "...", "csrftoken": "..." }, ... }`。
/// 旧 `cookies` 字段（单域）会从其中提取 www.yuketang.cn 部分填充，
/// 让回退到旧代码的版本仍能登录。
pub fn save_session(
    app_data_dir: &Path,
    username: &str,
    cookies_by_domain: HashMap<String, HashMap<String, String>>,
) -> Result<(), AppError> {
    let file_path = session_file_path(app_data_dir, username)?;

    // 向下兼容：旧字段填 www.yuketang.cn 域
    let legacy_cookies = cookies_by_domain
        .get("www.yuketang.cn")
        .or_else(|| cookies_by_domain.get("yuketang.cn"))
        .cloned()
        .unwrap_or_default();

    let data = SessionData {
        username: username.to_string(),
        cookies: legacy_cookies,
        cookies_by_domain,
        version: SESSION_VERSION,
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

    log::info!(
        "Session 已加密保存（{} 个域）: {}",
        data.cookies_by_domain.len(),
        file_path.display()
    );
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
        // 用旧 cookies 字段升级为多域结构
        let mut by_domain = HashMap::new();
        if !data.cookies.is_empty() {
            by_domain.insert("www.yuketang.cn".to_string(), data.cookies.clone());
        }
        save_session(app_data_dir, username, by_domain)?;
        let _ = std::fs::remove_file(&legacy);
        return load_session(app_data_dir, username);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_multi_domain() -> HashMap<String, HashMap<String, String>> {
        let mut m = HashMap::new();
        let mut yuketang = HashMap::new();
        yuketang.insert("sessionid".to_string(), "yk_session".to_string());
        yuketang.insert("csrftoken".to_string(), "yk_csrf".to_string());
        m.insert("www.yuketang.cn".to_string(), yuketang);

        let mut exam = HashMap::new();
        exam.insert("sessionid".to_string(), "exam_session".to_string());
        m.insert("examination.xuetangx.com".to_string(), exam);
        m
    }

    #[test]
    fn effective_falls_back_to_legacy_cookies() {
        let mut data = SessionData {
            username: "u".into(),
            cookies: HashMap::new(),
            cookies_by_domain: HashMap::new(),
            version: 1,
        };
        data.cookies.insert("sessionid".into(), "abc".into());
        let eff = data.effective_cookies_by_domain();
        assert_eq!(eff.len(), 1);
        assert_eq!(eff["www.yuketang.cn"]["sessionid"], "abc");
    }

    #[test]
    fn effective_prefers_new_field() {
        let by_domain = make_multi_domain();
        let mut data = SessionData {
            username: "u".into(),
            cookies: HashMap::from([("legacy".into(), "v".into())]),
            cookies_by_domain: by_domain.clone(),
            version: 2,
        };
        // 即使旧字段非空，新字段优先
        data.cookies.insert("legacy".into(), "v".into());
        let eff = data.effective_cookies_by_domain();
        assert_eq!(eff, by_domain);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = tempdir();
        let by_domain = make_multi_domain();
        save_session(&tmp, "测试用户", by_domain.clone()).unwrap();
        let loaded = load_session(&tmp, "测试用户").unwrap();
        assert_eq!(loaded.version, SESSION_VERSION);
        assert_eq!(loaded.cookies_by_domain, by_domain);
        // 旧字段也保留 yuketang.cn 那份
        assert_eq!(loaded.cookies["sessionid"], "yk_session");
    }

    #[test]
    fn legacy_format_deserializes() {
        // 没有 cookies_by_domain / version 字段也能读
        let json = r#"{"username":"u","cookies":{"sessionid":"abc"}}"#;
        let data: SessionData = serde_json::from_str(json).unwrap();
        assert_eq!(data.cookies["sessionid"], "abc");
        assert!(data.cookies_by_domain.is_empty());
        assert_eq!(data.version, 0);
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("raincourse-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
