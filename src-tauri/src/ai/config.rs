//! AI 模型配置的加密存储。
//!
//! 复用 `util::crypto`（与会话同一套 host-bound AES-256-GCM），把整个配置
//! （含 `api_key`）整体加密落盘到 `{app_data_dir}/ai_config.enc`。
//! 明文仅存在于内存与密文内，磁盘上不出现明文 API Key。

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::AppError;
use crate::util::crypto;

/// 加密配置文件名（放 app_data_dir 根，与 users/、answer/ 平级）。
const CONFIG_FILE: &str = "ai_config.enc";

/// AI 模型配置。OpenAI 兼容（base_url + api_key + model）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    /// OpenAI 兼容 base_url，如 `https://api.deepseek.com`（不含 `/v1`）。
    #[serde(default)]
    pub base_url: String,
    /// 模型名，如 `deepseek-chat`。
    #[serde(default)]
    pub model: String,
    /// 是否启用 AI 兜底。
    #[serde(default)]
    pub enabled: bool,
    /// API Key（落盘时随整个配置一起加密）。
    #[serde(default)]
    pub api_key: String,
}

impl AiConfig {
    /// 是否可用于调用：启用 + 三要素齐全。
    pub fn is_usable(&self) -> bool {
        self.enabled
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
            && !self.api_key.trim().is_empty()
    }
}

/// 保存 AI 配置（整体加密落盘）。
pub fn save_ai_config(app_data_dir: &Path, cfg: &AiConfig) -> Result<(), AppError> {
    let plaintext = serde_json::to_vec(cfg).map_err(AppError::Json)?;
    let encrypted = crypto::encrypt(app_data_dir, &plaintext)?;
    let path = app_data_dir.join(CONFIG_FILE);
    std::fs::write(&path, encrypted)?;

    // Unix 下进一步限制为 0600（owner only），与 session 保持一致。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 读取 AI 配置；文件不存在返回 `Ok(None)`。
pub fn load_ai_config(app_data_dir: &Path) -> Result<Option<AiConfig>, AppError> {
    let path = app_data_dir.join(CONFIG_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let container = std::fs::read_to_string(&path)?;
    let plaintext = crypto::decrypt(app_data_dir, &container)?;
    let cfg: AiConfig = serde_json::from_slice(&plaintext).map_err(AppError::Json)?;
    Ok(Some(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("raincourse-ai-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempdir();
        let cfg = AiConfig {
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-chat".into(),
            enabled: true,
            api_key: "sk-secret".into(),
        };
        save_ai_config(&dir, &cfg).unwrap();
        let loaded = load_ai_config(&dir).unwrap().unwrap();
        assert_eq!(loaded.base_url, cfg.base_url);
        assert_eq!(loaded.model, cfg.model);
        assert!(loaded.enabled);
        assert_eq!(loaded.api_key, "sk-secret");
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempdir();
        assert!(load_ai_config(&dir).unwrap().is_none());
    }

    #[test]
    fn is_usable_checks_all_fields() {
        let mut cfg = AiConfig::default();
        assert!(!cfg.is_usable());
        cfg.enabled = true;
        cfg.base_url = "x".into();
        cfg.model = "y".into();
        assert!(!cfg.is_usable()); // 缺 api_key
        cfg.api_key = "z".into();
        assert!(cfg.is_usable());
    }
}
