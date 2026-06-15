// 用户信息
export interface UserInfo {
  user_id: number;
  name: string;
  school?: string;
  /** 头像 URL，雨课堂返回字段名不一，可能是 head_image_url / avatar / head_url 等 */
  avatar?: string;
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
  /** 归属课程名（导出时写入；旧文件可能没有） */
  course_name?: string;
  /** 真实磁盘文件名（后端注入，用于删除定位） */
  file_name: string;
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

// AI 配置（保存用，含明文 api_key；从前端发往后端，留空表示不修改）
export interface AiConfig {
  base_url: string;
  model: string;
  enabled: boolean;
  api_key: string;
}

// AI 配置（读取用，后端屏蔽 api_key）
export interface AiConfigView {
  base_url: string;
  model: string;
  enabled: boolean;
  has_api_key: boolean;
}

// 自动答题逐题进度事件（quiz-answer-progress）
export interface QuizAnswerEvent {
  index: number;
  total: number;
  problem_id: string;
  /** preparing | running | done | failed | skipped */
  status: string;
  /** 答案来源：local（题库）| ai */
  source?: 'local' | 'ai' | null;
  is_correct?: boolean | null;
  message?: string | null;
}

// 自动答题汇总结果（命令返回 + quiz-answer-complete）
export interface QuizAnswerResult {
  total: number;
  submitted: number;
  correct: number;
  from_local: number;
  from_ai: number;
  failed: number;
  skipped: number;
  /** 已提交过而被跳过的题数（不重复作答） */
  already_answered: number;
}

// 课程学习进度（get_learn_schedule）；leaf_schedules 按 leaf_id 索引：1=完成、0=未完成、测验为浮点完成度
export interface LearnSchedule {
  leaf_schedules: Record<string, number>;
  total_schedule: number;
}

// 本地已导出测验的得分汇总（get_quiz_scores 返回 Record<string, QuizScore>，key=leaf_id）
export interface QuizScore {
  score: number;
  total: number;
  answered: number;
  count: number;
}

// 服务器权威成绩明细（get_score_detail 返回 Record<string, ScoreDetailItem>，key=leaf_id）。
// user_score=个人得分、leaf_score=满分，与雨课堂后台成绩单一致，优先于本地导出汇总展示。
export interface ScoreDetailItem {
  user_score: number;
  leaf_score: number;
}
