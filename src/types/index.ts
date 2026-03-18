// 用户信息
export interface UserInfo {
  user_id: number;
  name: string;
  school?: string;
}

// 课程
export interface Course {
  classroom_id: number;
  course_id: number;
  course_name: string;
  teacher_name: string;
}

// 作业
export interface Work {
  courseware_id: number;
  exam_id: string;
  title: string;
  status: string;
  score: number | null;
  problem_count: number | null;
  work_type: number;
}

// PPT 课件
export interface Ppt {
  courseware_id: number;
  title: string;
  count: number;
}

// 答案文件
export interface AnswerFile {
  exam_id: string;
  exam_name: string;
  exam_type: string;
}

// 导出结果
export interface ExportResult {
  answer_path: string | null;
  question_path: string | null;
  message: string;
}

// 章节任务
export interface TaskInfo {
  index: number;
  name: string;
  id: number;
  leaf_type: number;
  leaf_type_str: string;
  is_score: boolean;
  status: string;
  progress?: number;
  message?: string;
}

// 任务状态更新事件
export interface TaskUpdateEvent {
  index: number;
  total: number;
  name: string;
  type_str: string;
  status: string;
  progress?: number;
  message?: string;
}

// QR 码事件
export interface QrCodeEvent {
  url: string;
}

// 登录成功事件
export interface LoginSuccessEvent {
  user_id: number;
  name: string;
  school: string;
  auth: string;
}
