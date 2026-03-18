mod api;
mod commands;
mod error;
mod excel;
mod models;
mod session;
mod storage;
mod study;
mod ws;

use commands::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            client: Mutex::new(api::client::RainClient::new()),
            username: Mutex::new(None),
            study_cancel: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_client,
            commands::start_qr_login,
            commands::login_with_session,
            commands::get_saved_users,
            commands::get_user_info,
            commands::get_course_list,
            commands::get_course_works,
            commands::get_course_ppts,
            commands::export_work_answers,
            commands::export_exam_data,
            commands::get_answer_files,
            commands::get_course_chapters,
            commands::start_auto_study,
            commands::stop_auto_study,
            commands::get_chapter_tasks,
            commands::export_excel,
            commands::get_exam_files,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
