mod ai;
mod api;
mod commands;
mod error;
mod excel;
mod models;
mod session;
mod storage;
mod study;
mod util;
mod ws;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 测试环境会重复 init，try_init 避免 panic
    let _ = env_logger::try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::init_client,
            commands::start_qr_login,
            commands::login_with_session,
            commands::get_saved_users,
            commands::remove_saved_user,
            commands::get_user_info,
            commands::clear_session,
            commands::get_course_list,
            commands::get_course_works,
            commands::get_course_ppts,
            commands::export_work_answers,
            commands::export_quiz_answers,
            commands::export_exam_data,
            commands::get_answer_files,
            commands::get_course_chapters,
            commands::start_auto_study,
            commands::stop_auto_study,
            commands::get_chapter_tasks,
            commands::export_excel,
            commands::get_exam_files,
            commands::save_ai_config,
            commands::get_ai_config,
            commands::start_quiz_auto_answer,
            commands::stop_quiz_auto_answer,
            commands::get_learn_schedule,
            commands::get_quiz_scores,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
