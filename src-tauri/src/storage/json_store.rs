use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::util::sanitize;

/// 答案分享包的格式标识，导入时据此校验文件确为本应用导出的分享包。
const BUNDLE_FORMAT: &str = "raincourse-answers";

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

/// 列出答案文件。每条返回文件内的 `info`，并注入真实磁盘文件名 `file_name`，
/// 供前端精确定位删除（answer 目录下文件名格式不一：`{id}.json` / `quiz_*.json` / `ppt_*.json`）。
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
                                let mut info = info.clone();
                                // 用真实磁盘文件名覆盖注入，作为删除时的可靠定位键
                                if let Some(obj) = info.as_object_mut() {
                                    obj.insert(
                                        "file_name".to_string(),
                                        Value::String(name.to_string()),
                                    );
                                }
                                files.push(info);
                            }
                        }
                    }
                }
            }
        }
    }

    files
}

/// 删除 answer 目录下指定答案文件。
///
/// 文件名经 [`sanitize::sanitize_filename`] 清洗并二次校验落在 answer 目录内，
/// 防止路径穿越；文件不存在视为成功（幂等，便于批量删除中并发刷新的容错）。
pub fn delete_answer_file(app_data_dir: &Path, file_name: &str) -> Result<(), AppError> {
    let dir = get_answer_dir(app_data_dir)?;
    let stem = file_name.strip_suffix(".json").unwrap_or(file_name);
    let clean = sanitize::sanitize_filename(stem)?;
    let file_path = dir.join(format!("{}.json", clean));
    if !file_path.starts_with(&dir) {
        return Err(AppError::InvalidInput(format!("非法文件名: {}", file_name)));
    }
    match std::fs::remove_file(&file_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// 把指定答案文件打包成单个可分享的 bundle，写入 `dest_path`（用户经对话框选定）。
///
/// 每个条目保留**真实磁盘文件名**与完整内容（`{answer, info}`），便于导入端原样还原。
/// 缺失/损坏的文件静默跳过；返回实际打入的条目数。`file_names` 来自
/// [`list_answer_files`] 注入的 `file_name`，已是 answer 目录内的真实文件名。
pub fn export_answer_bundle(
    app_data_dir: &Path,
    file_names: &[String],
    dest_path: &Path,
) -> Result<usize, AppError> {
    let dir = get_answer_dir(app_data_dir)?;
    let mut items: Vec<Value> = Vec::new();

    for name in file_names {
        // load_json 已清洗文件名并二次校验落在 answer 目录内
        match load_json(&dir, name) {
            Ok(content) => items.push(serde_json::json!({
                "file_name": name,
                "content": content,
            })),
            // 缺失/损坏不阻断整体导出，直接跳过
            Err(e) => log::warn!("导出答案分享包时跳过 {}: {}", name, e),
        }
    }

    let bundle = serde_json::json!({
        "format": BUNDLE_FORMAT,
        "version": 1,
        "exported_at": chrono::Local::now().to_rfc3339(),
        "count": items.len(),
        "items": items,
    });

    let json = serde_json::to_string_pretty(&bundle)?;
    std::fs::write(dest_path, json)?;
    Ok(items.len())
}

/// 从 `src_path`（用户经对话框选定）的分享包导入答案文件到 answer 目录。
///
/// - 校验 `format` 字段确为本应用导出的分享包，否则报错；
/// - 每个条目的文件名经清洗并二次校验落在 answer 目录内，防止恶意分享包穿越写盘；
/// - 内容须含 `answer` 与 `info` 两段，否则计入失败；
/// - **已存在同名文件则跳过**（不覆盖）。
///
/// 返回 `{ imported, skipped, failed: [{file_name, reason}] }`。
pub fn import_answer_bundle(app_data_dir: &Path, src_path: &Path) -> Result<Value, AppError> {
    let content = std::fs::read_to_string(src_path)?;
    let bundle: Value = serde_json::from_str(&content)?;

    if bundle["format"].as_str() != Some(BUNDLE_FORMAT) {
        return Err(AppError::InvalidInput(
            "无法识别的答案分享文件（格式不匹配，请选择本应用导出的分享包）".into(),
        ));
    }

    let dir = get_answer_dir(app_data_dir)?;
    let items = bundle["items"].as_array().cloned().unwrap_or_default();

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<Value> = Vec::new();

    for item in &items {
        let raw_name = item["file_name"].as_str().unwrap_or("");
        let item_content = &item["content"];

        // 结构校验：必须含 answer 与 info（与 save_json 落盘结构一致）
        if raw_name.is_empty()
            || !item_content.is_object()
            || item_content.get("answer").is_none()
            || item_content.get("info").is_none()
        {
            failed.push(serde_json::json!({
                "file_name": raw_name,
                "reason": "条目结构无效（缺少 answer 或 info）",
            }));
            continue;
        }

        // 清洗文件名并二次校验落在 answer 目录内
        let stem = raw_name.strip_suffix(".json").unwrap_or(raw_name);
        let clean = match sanitize::sanitize_filename(stem) {
            Ok(c) => c,
            Err(e) => {
                failed.push(serde_json::json!({
                    "file_name": raw_name,
                    "reason": e.to_string(),
                }));
                continue;
            }
        };
        let file_path = dir.join(format!("{}.json", clean));
        if !file_path.starts_with(&dir) {
            failed.push(serde_json::json!({
                "file_name": raw_name,
                "reason": "非法文件名",
            }));
            continue;
        }

        // 已存在则跳过（不覆盖本地答案）
        if file_path.exists() {
            skipped += 1;
            continue;
        }

        let json = match serde_json::to_string_pretty(item_content) {
            Ok(j) => j,
            Err(e) => {
                failed.push(serde_json::json!({
                    "file_name": raw_name,
                    "reason": e.to_string(),
                }));
                continue;
            }
        };
        match std::fs::write(&file_path, json) {
            Ok(()) => imported += 1,
            Err(e) => failed.push(serde_json::json!({
                "file_name": raw_name,
                "reason": e.to_string(),
            })),
        }
    }

    Ok(serde_json::json!({
        "imported": imported,
        "skipped": skipped,
        "failed": failed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个临时 app_data_dir，并在 answer 目录写入两个答案文件。
    fn setup_with_answers(tmp: &Path) {
        let dir = get_answer_dir(tmp).unwrap();
        let a = serde_json::json!({
            "answer": {"q1": "A"},
            "info": {"exam_id": "111", "exam_name": "作业一", "exam_type": "考试试题"}
        });
        let b = serde_json::json!({
            "answer": {"q1": "B"},
            "info": {"exam_id": "222", "exam_name": "测验二", "exam_type": "章节测验"}
        });
        std::fs::write(
            dir.join("111.json"),
            serde_json::to_string_pretty(&a).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("quiz_222.json"),
            serde_json::to_string_pretty(&b).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn export_then_import_roundtrip_skips_existing() {
        let src = std::env::temp_dir().join(format!("rc_src_{}", std::process::id()));
        let dst = std::env::temp_dir().join(format!("rc_dst_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);

        setup_with_answers(&src);
        let bundle_path = src.join("bundle.json");

        // 导出两个文件
        let n = export_answer_bundle(
            &src,
            &["111.json".to_string(), "quiz_222.json".to_string()],
            &bundle_path,
        )
        .unwrap();
        assert_eq!(n, 2);

        // 导入到一个全新的 app_data_dir：两个都应成功
        let res = import_answer_bundle(&dst, &bundle_path).unwrap();
        assert_eq!(res["imported"], 2);
        assert_eq!(res["skipped"], 0);
        assert!(res["failed"].as_array().unwrap().is_empty());

        // 再次导入：均已存在 → 全部跳过
        let res2 = import_answer_bundle(&dst, &bundle_path).unwrap();
        assert_eq!(res2["imported"], 0);
        assert_eq!(res2["skipped"], 2);

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn import_rejects_wrong_format() {
        let dst = std::env::temp_dir().join(format!("rc_bad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(&dst).unwrap();
        let bad = dst.join("bad.json");
        std::fs::write(&bad, r#"{"format":"something-else","items":[]}"#).unwrap();

        let err = import_answer_bundle(&dst, &bad).unwrap_err();
        assert_eq!(err.code(), "INVALID_INPUT");

        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn import_skips_traversal_and_invalid_items() {
        let dst = std::env::temp_dir().join(format!("rc_trav_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(&dst).unwrap();
        let bundle_path = dst.join("b.json");

        let bundle = serde_json::json!({
            "format": BUNDLE_FORMAT,
            "version": 1,
            "items": [
                // 路径穿越尝试：清洗后仅落在 answer 目录内，不会写到上级
                {"file_name": "../../evil.json",
                 "content": {"answer": {}, "info": {"exam_id": "9"}}},
                // 结构无效：缺少 info
                {"file_name": "broken.json", "content": {"answer": {}}},
            ]
        });
        std::fs::write(&bundle_path, serde_json::to_string(&bundle).unwrap()).unwrap();

        let res = import_answer_bundle(&dst, &bundle_path).unwrap();
        // 穿越项被清洗后落在 answer 目录内、结构有效 → 计 1 个导入
        assert_eq!(res["imported"], 1);
        // 结构无效项计入失败
        assert_eq!(res["failed"].as_array().unwrap().len(), 1);
        // 确认上级目录没有被写入 evil.json
        assert!(!dst.parent().unwrap().join("evil.json").exists());

        let _ = std::fs::remove_dir_all(&dst);
    }
}
