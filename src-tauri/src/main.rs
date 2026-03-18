// 为桌面构建提供 main 入口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    raincourse_v2_lib::run();
}
