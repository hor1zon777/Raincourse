//! 会话凭据加密：AES-256-GCM + SHA-256 派生 key
//!
//! **安全说明**：这不是密码学高强度的方案，目的是让 sessionid 不再以
//! 明文落盘，本机其它进程无法直接 grep 出 `sessionid=xxx` 复用。
//!
//! Key 派生：`SHA-256(app_salt || hostname || app_data_dir)`
//! - 文件复制到另一台机器（hostname 不同）后无法解密
//! - 不依赖外部 keyring 服务，跨平台一致
//!
//! 真正想要"OS-bound"安全请改用 `tauri-plugin-stronghold` /
//! Windows DPAPI / macOS Keychain / Linux Secret Service。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::error::AppError;

/// 应用固定 salt（修改后旧凭据无法解密）。
const APP_SALT: &[u8] = b"raincourse-v2/session-v1/2026-05";

/// 加密后写入磁盘的容器格式。
#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedBlob {
    /// 算法版本，后续如换密钥派生可平滑迁移。
    pub v: u8,
    /// 12 字节 nonce（base64）
    pub n: String,
    /// 密文 + GCM tag（base64）
    pub c: String,
}

const VERSION: u8 = 1;

/// 派生加密 key。
fn derive_key(app_data_dir: &Path) -> [u8; 32] {
    let hostname = hostname::get()
        .ok()
        .and_then(|s| s.into_string().ok())
        .unwrap_or_else(|| "unknown-host".to_string());

    let mut hasher = Sha256::new();
    hasher.update(APP_SALT);
    hasher.update(hostname.as_bytes());
    hasher.update(app_data_dir.to_string_lossy().as_bytes());
    hasher.finalize().into()
}

/// 加密任意 bytes，返回 JSON 序列化后的容器字符串。
pub fn encrypt(app_data_dir: &Path, plaintext: &[u8]) -> Result<String, AppError> {
    let key_bytes = derive_key(app_data_dir);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 生成随机 nonce
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::General(format!("加密失败: {}", e)))?;

    let blob = EncryptedBlob {
        v: VERSION,
        n: base64_encode(&nonce_bytes),
        c: base64_encode(&ciphertext),
    };
    serde_json::to_string(&blob).map_err(AppError::Json)
}

/// 解密容器字符串，返回明文 bytes。
pub fn decrypt(app_data_dir: &Path, container: &str) -> Result<Vec<u8>, AppError> {
    let blob: EncryptedBlob = serde_json::from_str(container).map_err(AppError::Json)?;

    if blob.v != VERSION {
        return Err(AppError::General(format!("不支持的凭据版本: {}", blob.v)));
    }

    let nonce_bytes = base64_decode(&blob.n)?;
    let ciphertext = base64_decode(&blob.c)?;

    if nonce_bytes.len() != 12 {
        return Err(AppError::General("nonce 长度异常".to_string()));
    }

    let key_bytes = derive_key(app_data_dir);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| AppError::General(format!("解密失败（凭据可能损坏或换了机器）: {}", e)))
}

// ---- 简易 base64（避免再引入 base64 crate）----
const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push(B64_CHARS[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>, AppError> {
    fn idx(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return Err(AppError::General("base64 长度非法".to_string()));
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let pad = chunk.iter().rev().take_while(|&&b| b == b'=').count();
        let v0 = idx(chunk[0]).ok_or_else(|| AppError::General("base64 非法字符".into()))?;
        let v1 = idx(chunk[1]).ok_or_else(|| AppError::General("base64 非法字符".into()))?;
        let v2 = if pad >= 2 {
            0
        } else {
            idx(chunk[2]).ok_or_else(|| AppError::General("base64 非法字符".into()))?
        };
        let v3 = if pad >= 1 {
            0
        } else {
            idx(chunk[3]).ok_or_else(|| AppError::General("base64 非法字符".into()))?
        };
        let n = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        out.push(((n >> 16) & 0xFF) as u8);
        if pad < 2 {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if pad < 1 {
            out.push((n & 0xFF) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = Path::new("/tmp/app");
        let plain = b"hello sessionid=abc; csrftoken=xyz";
        let ct = encrypt(dir, plain).unwrap();
        let pt = decrypt(dir, &ct).unwrap();
        assert_eq!(pt, plain);
    }

    #[test]
    fn different_dir_fails() {
        let dir1 = Path::new("/tmp/app1");
        let dir2 = Path::new("/tmp/app2");
        let ct = encrypt(dir1, b"secret").unwrap();
        assert!(decrypt(dir2, &ct).is_err());
    }

    #[test]
    fn b64_roundtrip() {
        for &case in &["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            let enc = base64_encode(case.as_bytes());
            let dec = base64_decode(&enc).unwrap();
            assert_eq!(dec, case.as_bytes(), "case={:?}", case);
        }
    }
}
