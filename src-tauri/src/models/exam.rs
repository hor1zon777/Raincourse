use serde::{Deserialize, Serialize};

/// 作业/考试状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkStatus {
    NotSubmitted,
    NotSubmittedRed,
    Corrected,
    CorrectedGreen,
    DoneCommit,
    NotCommit,
    Unknown(i32),
}

impl WorkStatus {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::NotSubmitted,
            1 => Self::NotSubmittedRed,
            2 => Self::Corrected,
            3 => Self::CorrectedGreen,
            5 => Self::DoneCommit,
            6 => Self::NotCommit,
            other => Self::Unknown(other),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::NotSubmitted | Self::NotSubmittedRed => "未提交",
            Self::Corrected | Self::CorrectedGreen => "已批改",
            Self::DoneCommit => "已提交",
            Self::NotCommit => "缺考",
            Self::Unknown(_) => "未知",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Work {
    pub courseware_id: i64,
    pub title: String,
    pub status: String,
    pub score: Option<f64>,
    pub problem_count: Option<i32>,
    pub work_type: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ppt {
    pub courseware_id: i64,
    pub title: String,
    pub count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerFile {
    pub exam_id: String,
    pub exam_name: String,
    pub exam_type: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExamFile {
    pub exam_id: String,
    pub exam_name: String,
    pub has_question: bool,
    pub has_answer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub answer_path: Option<String>,
    pub question_path: Option<String>,
    pub message: String,
}
