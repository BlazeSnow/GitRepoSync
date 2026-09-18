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
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("解析数据目录失败: {e}"))?;
            let state = Arc::new(AppState::init(data_dir)?);
            let port = lock_port(&state);
            mcp::start_server(state.clone(), port);
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
            settings::get_mcp_config,
            settings::set_mcp_port,
            settings::regenerate_mcp_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("Git Repo Sync 启动失败");
}

fn lock_port(state: &Arc<AppState>) -> u16 {
    state::lock(&state.settings).mcp_port
}
