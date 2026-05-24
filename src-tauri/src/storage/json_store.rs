use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::util::sanitize;

/// 获取 answer 目录（按需创建）。
pub fn get_answer_dir(app_data_dir: &Path) -> Result<PathBuf, AppError> {
    let dir = app_data_dir.join("answer");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// 获取 exam 目录（按需创建）。
pub fn get_exam_dir(app_data_dir: &Path) -> Result<PathBuf, AppError> {
    let dir = app_data_dir.join("exam");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// 保存 JSON 数据到文件（文件名会被清洗，防止路径穿越）。
pub fn save_json(
    dir: &Path,
    filename: &str,
    data: &Value,
    info: &Value,
) -> Result<String, AppError> {
    let clean = sanitize::sanitize_filename(filename)?;
    let file_path = dir.join(format!("{}.json", clean));
    if !file_path.starts_with(dir) {
        return Err(AppError::InvalidInput(format!("非法文件名: {}", filename)));
    }

    let combined = serde_json::json!({
        "answer": data,
        "info": info,
    });
    let json = serde_json::to_string_pretty(&combined)?;
    std::fs::write(&file_path, json)?;
    Ok(file_path.to_string_lossy().to_string())
}

/// 从 JSON 文件读取数据（文件名会被清洗）。
pub fn load_json(dir: &Path, filename: &str) -> Result<Value, AppError> {
    // 这里 filename 形如 "1234_question.json"，扩展名也是可信的
    // 但 stem 仍来自调用方，需要清洗
    let stem = filename.strip_suffix(".json").unwrap_or(filename);
    let clean = sanitize::sanitize_filename(stem)?;
    let file_path = dir.join(format!("{}.json", clean));
    if !file_path.starts_with(dir) {
        return Err(AppError::InvalidInput(format!("非法文件名: {}", filename)));
    }

    let content = std::fs::read_to_string(&file_path)?;
    let data: Value = serde_json::from_str(&content)?;
    Ok(data)
}

/// 列出答案文件。
pub fn list_answer_files(app_data_dir: &Path) -> Vec<Value> {
    let dir = match get_answer_dir(app_data_dir) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(data) = serde_json::from_str::<Value>(&content) {
                            if let Some(info) = data.get("info") {
                                files.push(info.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    files
}
