use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterData {
    pub course_chapter: Vec<Chapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub name: Option<String>,
    pub section_leaf_list: Vec<SectionOrLeaf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionOrLeaf {
    pub id: i64,
    pub name: Option<String>,
    pub leaf_type: Option<i32>,
    pub leafinfo_id: Option<i64>,
    pub is_score: Option<bool>,
    pub is_locked: Option<bool>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub score_deadline: Option<i64>,
    pub leaf_list: Option<Vec<LeafItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeafItem {
    pub id: i64,
    pub name: Option<String>,
    pub leaf_type: Option<i32>,
    pub leafinfo_id: Option<i64>,
    pub is_score: Option<bool>,
    pub is_locked: Option<bool>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub score_deadline: Option<i64>,
}

/// 前端展示用的任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub index: usize,
    pub name: String,
    pub id: i64,
    pub leaf_type: i32,
    pub leaf_type_str: String,
    pub is_score: bool,
    pub status: String,
    pub progress: Option<f64>,
    pub message: Option<String>,
}

pub fn leaf_type_label(leaf_type: i32) -> &'static str {
    match leaf_type {
        0 => "视频",
        3 => "公告",
        4 => "讨论",
        6 => "测验/练习",
        _ => "未知",
    }
}
