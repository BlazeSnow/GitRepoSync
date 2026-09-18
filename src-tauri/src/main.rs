#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod mcp;
mod providers;
mod repos;
mod settings;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // mcp 子命令：以 stdio 模式运行 MCP 服务（供 Agent 客户端作为子进程拉起）
    if args.first().map(String::as_str) == Some("mcp") {
        run_mcp_stdio(&args);
        return;
    }

    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("解析数据目录失败: {e}"))?;
            let state = Arc::new(AppState::open(data_dir.join("app.db"))?);
            state.reset_running_repos();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth::login,
            auth::restore_session,
            auth::logout,
            auth::change_password,
            repos::list_repos,
            repos::save_repo,
            repos::delete_repo,
            repos::start_sync,
            repos::stop_sync,
            providers::get_providers,
            providers::save_provider,
            providers::fetch_provider_accounts,
            settings::get_app_info,
            settings::get_base_dir,
            settings::set_base_dir,
            settings::get_mcp_config,
            settings::regenerate_mcp_api_key,
            settings::list_operation_logs,
        ])
        .run(tauri::generate_context!())
        .expect("Git Repo Sync 启动失败");
}

fn run_mcp_stdio(args: &[String]) {
    // APIKEY：优先 --api-key 参数，其次环境变量 GIT_REPO_SYNC_API_KEY
    let mut provided = std::env::var("GIT_REPO_SYNC_API_KEY").ok();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--api-key" {
            if let Some(v) = args.get(i + 1) {
                provided = Some(v.clone());
            }
            break;
        }
        i += 1;
    }

    // 与 Tauri 运行时的 app_data_dir 保持一致：系统数据目录 + 应用标识符
    let data_dir = dirs::data_dir()
        .expect("无法定位系统数据目录")
        .join("com.blazesnow.gitreposync");
    let state = Arc::new(AppState::open(data_dir.join("app.db")).expect("初始化数据库失败"));
    mcp::run_stdio(state.clone(), provided);
    // stdin 已关闭（客户端断开）：等待在途同步完成后再退出，避免中断同步
    state.wait_syncs_idle();
}
