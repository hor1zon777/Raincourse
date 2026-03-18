use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub classroom_id: i64,
    pub course_id: i64,
    pub course_name: String,
    pub teacher_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseRaw {
    pub course: CourseInner,
    pub teacher: TeacherInner,
    pub classroom_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseInner {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeacherInner {
    pub name: String,
}
