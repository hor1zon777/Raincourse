//! 文件名 / 路径清洗工具
//!
//! 防止用户名、考试名等不可信字符串造成路径穿越、文件覆盖。

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// 文件名禁用字符（Windows + Unix 通用）。
const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];

/// 清洗单个文件名（不包含目录分隔符）。
///
/// - 移除控制字符与禁用字符
/// - 把 `..` / `.` 单段名替换为 `_`
/// - 限制长度，避免文件系统超限
/// - 空串返回错误
pub fn sanitize_filename(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("文件名不能为空".to_string()));
    }

    let mut cleaned: String = trimmed
        .chars()
        .map(|c| {
            if c.is_control() || INVALID_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();

    // 防止 ".." / "." 单段名穿越上级
    if cleaned == ".." || cleaned == "." {
        cleaned = "_".to_string();
    }

    // 去掉首尾的点和空格（Windows 不允许）
    cleaned = cleaned
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    if cleaned.is_empty() {
        return Err(AppError::InvalidInput("文件名无有效字符".to_string()));
    }

    // 限制长度（保留扩展空间）
    const MAX_LEN: usize = 120;
    if cleaned.chars().count() > MAX_LEN {
        cleaned = cleaned.chars().take(MAX_LEN).collect();
    }

    Ok(cleaned)
}

/// 把清洗后的文件名安全地拼接到 base_dir 下，并二次校验 `final.starts_with(base_dir)`。
pub fn safe_join(base_dir: &Path, filename: &str) -> Result<PathBuf, AppError> {
    let clean = sanitize_filename(filename)?;
    let candidate = base_dir.join(&clean);

    // 规范化前先做字面前缀校验
    if !candidate.starts_with(base_dir) {
        return Err(AppError::InvalidInput(format!("非法路径: {}", filename)));
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("   ").is_err());
    }

    #[test]
    fn replaces_separators() {
        assert_eq!(sanitize_filename("a/b\\c").unwrap(), "a_b_c");
    }

    #[test]
    fn rejects_dot_only() {
        assert_eq!(sanitize_filename("..").unwrap(), "_");
        assert_eq!(sanitize_filename(".").unwrap(), "_");
    }

    #[test]
    fn strips_trailing_dots() {
        assert_eq!(sanitize_filename("abc.").unwrap(), "abc");
        assert_eq!(sanitize_filename("abc...").unwrap(), "abc");
    }

    #[test]
    fn preserves_chinese() {
        assert_eq!(sanitize_filename("张三_测试").unwrap(), "张三_测试");
    }

    #[test]
    fn truncates_long() {
        let long = "a".repeat(200);
        let cleaned = sanitize_filename(&long).unwrap();
        assert!(cleaned.chars().count() <= 120);
    }

    #[test]
    fn safe_join_blocks_traversal() {
        let base = Path::new("/tmp/app");
        // sanitize_filename 已把 / 替换为 _，所以最终路径不会跨级
        let joined = safe_join(base, "../etc/passwd").unwrap();
        assert!(joined.starts_with(base));
    }
}
