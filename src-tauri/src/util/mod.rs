pub mod crypto;
pub mod sanitize;

use serde_json::Value;

/// 把 JSON 值取成字符串，兼容「数字或字符串」两种类型；null 返回空串。
///
/// 雨课堂部分字段（如 leaf_type_id / sku_id）在不同响应里可能是数字或字符串，
/// 与 `export_work_answers` 对 `user_id` 的兼容处理保持一致。
pub fn json_str_or_num(v: &Value) -> String {
    match v.as_str() {
        Some(s) => s.to_string(),
        None if v.is_null() => String::new(),
        None => v.to_string().replace('"', ""),
    }
}
