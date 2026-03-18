use serde_json::Value;
use std::path::PathBuf;

use crate::error::AppError;

/// 获取 answer 目录
pub fn get_answer_dir(app_data_dir: &PathBuf) -> PathBuf {
    let dir = app_data_dir.join("answer");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
    }
    dir
}

/// 获取 exam 目录
pub fn get_exam_dir(app_data_dir: &PathBuf) -> PathBuf {
    let dir = app_data_dir.join("exam");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
    }
    dir
}

/// 保存 JSON 数据到文件
pub fn save_json(
    dir: &PathBuf,
    filename: &str,
    data: &Value,
    info: &Value,
) -> Result<String, AppError> {
    let file_path = dir.join(format!("{}.json", filename));
    let combined = serde_json::json!({
        "answer": data,
        "info": info,
    });
    let json = serde_json::to_string_pretty(&combined)?;
    std::fs::write(&file_path, json)?;
    Ok(file_path.to_string_lossy().to_string())
}

/// 从 JSON 文件读取数据
pub fn load_json(dir: &PathBuf, filename: &str) -> Result<Value, AppError> {
    let file_path = dir.join(filename);
    let content = std::fs::read_to_string(&file_path)?;
    let data: Value = serde_json::from_str(&content)?;
    Ok(data)
}

/// 检查答案文件是否存在
pub fn answer_file_exists(app_data_dir: &PathBuf, filename: &str) -> bool {
    let dir = get_answer_dir(app_data_dir);
    dir.join(filename).exists()
}

/// 列出答案文件
pub fn list_answer_files(app_data_dir: &PathBuf) -> Vec<Value> {
    let dir = get_answer_dir(app_data_dir);
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
