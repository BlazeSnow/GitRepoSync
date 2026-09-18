use crate::auth::require_session;
use crate::state::{lock, AppState};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
}

#[tauri::command]
pub fn get_app_info(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<AppInfo, String> {
    require_session(&state, &token)?;
    let pkg = app.package_info();
    Ok(AppInfo {
        name: pkg.name.clone(),
        version: pkg.version.to_string(),
        os: std::env::consts::OS.into(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    pub api_key: String,
    pub exe_path: String,
}

#[tauri::command]
pub fn get_mcp_config(state: State<'_, Arc<AppState>>, token: String) -> Result<McpConfig, String> {
    require_session(&state, &token)?;
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let s = lock(&state.settings);
    Ok(McpConfig {
        api_key: s.mcp_api_key.clone(),
        exe_path,
    })
}

#[tauri::command]
pub fn regenerate_mcp_api_key(
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<String, String> {
    require_session(&state, &token)?;
    let key = format!("grs_{}", Uuid::new_v4().simple());
    {
        lock(&state.settings).mcp_api_key = key.clone();
    }
    state.save_settings()?;
    Ok(key)
}
