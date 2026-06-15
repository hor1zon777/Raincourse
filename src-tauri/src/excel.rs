use std::path::Path;

use rust_xlsxwriter::{Format, Workbook, XlsxError};
use serde_json::Value;

use crate::error::AppError;
use crate::storage::json_store;
use crate::util::sanitize;

/// 从 HTML 中移除标签
fn strip_html(input: &str) -> String {
    let mut result = String::new();
    let mut inside_tag = false;
    for ch in input.chars() {
        if ch == '<' {
            inside_tag = true;
        } else if ch == '>' {
            inside_tag = false;
        } else if !inside_tag {
            result.push(ch);
        }
    }
    // 清理多余空白
    result
        .replace('\n', " ")
        .replace('\r', " ")
        .replace('\t', " ")
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// 根据 problem_id 从答案列表中查找答案
fn find_answer_by_id<'a>(problem_id: &str, results: &'a [Value]) -> Option<&'a Value> {
    results
        .iter()
        .find(|r| r["problem_id"].as_str() == Some(problem_id))
        .map(|r| &r["answer"])
}

/// 处理考试数据并生成 Excel
pub fn export_exam_excel(
    app_data_dir: &Path,
    exam_id: &str,
    exam_name: &str,
) -> Result<String, AppError> {
    // 清洗 exam_id（来自前端，会拼到文件名）
    let exam_id_clean = sanitize::sanitize_filename(exam_id)?;
    let exam_dir = json_store::get_exam_dir(app_data_dir)?;

    // 读取题目和答案文件
    let question_data =
        json_store::load_json(&exam_dir, &format!("{}_question.json", exam_id_clean))?;
    let answer_data = json_store::load_json(&exam_dir, &format!("{}_answer.json", exam_id_clean))?;

    let problems = question_data["exam"]["data"]["problems"]
        .as_array()
        .ok_or_else(|| AppError::General("题目数据格式错误".into()))?;

    let problem_results = answer_data["exam"]["data"]["problem_results"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // 创建工作簿
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    // Excel sheet 名限制：≤31 字符 + 禁用 :/\?*[]
    let sheet_name: String = sanitize::sanitize_filename(exam_name)
        .unwrap_or_else(|_| exam_id_clean.clone())
        .chars()
        .map(|c| match c {
            ':' | '/' | '\\' | '?' | '*' | '[' | ']' => '_',
            _ => c,
        })
        .take(31)
        .collect();
    worksheet
        .set_name(&sheet_name)
        .map_err(|e: XlsxError| AppError::General(e.to_string()))?;

    // 设置表头格式
    let header_fmt = Format::new().set_bold();

    let headers = [
        "题型",
        "试题标题",
        "题目",
        "选项A",
        "选项B",
        "选项C",
        "选项D",
        "选项E",
        "选项F",
        "正确答案",
        "解析",
    ];
    for (col, header) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col as u16, *header, &header_fmt)
            .map_err(|e| AppError::General(e.to_string()))?;
    }

    // 写入数据
    for (row_idx, question) in problems.iter().enumerate() {
        let row = (row_idx + 1) as u32;
        let type_text = question["TypeText"].as_str().unwrap_or("未知");
        let problem_id = question["problem_id"].as_str().unwrap_or("");

        // 题型编号
        let type_code = match type_text {
            "单选题" => "1",
            "多选题" => "2",
            "判断题" => "3",
            "填空题" => "4",
            _ => type_text,
        };

        // 题目内容
        let body = strip_html(question["Body"].as_str().unwrap_or(""));
        let remark = strip_html(question["Remark"].as_str().unwrap_or(""));

        // 选项
        let options = question["Options"].as_array();
        let option_texts: Vec<String> = options
            .map(|opts| {
                opts.iter()
                    .map(|o| strip_html(o["value"].as_str().unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();

        // 查找答案
        let raw_answer = find_answer_by_id(problem_id, &problem_results);
        let answer_display = format_answer(type_text, raw_answer, options);

        // 写入行
        worksheet
            .write_string(row, 0, type_code)
            .map_err(|e| AppError::General(e.to_string()))?;
        worksheet
            .write_string(row, 1, &sheet_name)
            .map_err(|e| AppError::General(e.to_string()))?;
        worksheet
            .write_string(row, 2, &body)
            .map_err(|e| AppError::General(e.to_string()))?;

        for (i, opt) in option_texts.iter().enumerate() {
            if i < 6 {
                worksheet
                    .write_string(row, (3 + i) as u16, opt)
                    .map_err(|e| AppError::General(e.to_string()))?;
            }
        }
        // 空选项填空
        for i in option_texts.len()..6 {
            worksheet
                .write_string(row, (3 + i) as u16, "")
                .map_err(|e| AppError::General(e.to_string()))?;
        }

        worksheet
            .write_string(row, 9, &answer_display)
            .map_err(|e| AppError::General(e.to_string()))?;
        worksheet
            .write_string(row, 10, &remark)
            .map_err(|e| AppError::General(e.to_string()))?;
    }

    // 保存文件（exam_name 已清洗）
    let excel_dir = app_data_dir.join("excel");
    if !excel_dir.exists() {
        std::fs::create_dir_all(&excel_dir)?;
    }
    let safe_name =
        sanitize::sanitize_filename(exam_name).unwrap_or_else(|_| exam_id_clean.clone());
    let file_path = excel_dir.join(format!("{}.xlsx", safe_name));
    if !file_path.starts_with(&excel_dir) {
        return Err(AppError::InvalidInput(format!("非法文件名: {}", exam_name)));
    }
    let path_str = file_path.to_string_lossy().to_string();

    workbook
        .save(&path_str)
        .map_err(|e| AppError::General(format!("保存 Excel 失败: {}", e)))?;

    Ok(path_str)
}

/// 格式化答案显示
fn format_answer(type_text: &str, raw: Option<&Value>, options: Option<&Vec<Value>>) -> String {
    match (type_text, raw) {
        ("单选题" | "多选题", Some(answer)) => {
            if let Some(arr) = answer.as_array() {
                let opts = options.cloned().unwrap_or_default();
                arr.iter()
                    .filter_map(|key| {
                        let key_str = key.as_str()?;
                        let idx = opts
                            .iter()
                            .position(|o| o["key"].as_str() == Some(key_str))?;
                        Some((b'A' + idx as u8) as char)
                    })
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                "答案格式异常".into()
            }
        }
        ("判断题", Some(answer)) => {
            if let Some(arr) = answer.as_array() {
                if let Some(first) = arr.first() {
                    match first.as_str().unwrap_or("").to_lowercase().as_str() {
                        "true" => "A".into(),
                        "false" => "B".into(),
                        other => other.to_uppercase(),
                    }
                } else {
                    "答案缺失".into()
                }
            } else {
                "答案格式异常".into()
            }
        }
        ("填空题", Some(answer)) => {
            if let Some(obj) = answer.as_object() {
                let mut keys: Vec<&String> = obj.keys().collect();
                keys.sort_by(|a, b| {
                    a.parse::<i32>()
                        .unwrap_or(0)
                        .cmp(&b.parse::<i32>().unwrap_or(0))
                });
                keys.iter()
                    .map(|k| {
                        let val = &obj[*k];
                        if let Some(arr) = val.as_array() {
                            format!("【{}】", arr.first().and_then(|v| v.as_str()).unwrap_or(""))
                        } else if let Some(s) = val.as_str() {
                            format!("【{}】", s)
                        } else {
                            "【】".into()
                        }
                    })
                    .collect::<String>()
            } else {
                "答案格式异常".into()
            }
        }
        (_, None) => "答案未获取".into(),
        _ => "未知题型".into(),
    }
}
