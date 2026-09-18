use crate::auth::require_session;
use crate::state::{lock, AppState, OperationLog, DEFAULT_BASE_DIR};
use rusqlite::params;
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

/// 仓库本地基地址（中转站），默认 ~/repo
#[tauri::command]
pub fn get_base_dir(state: State<'_, Arc<AppState>>, token: String) -> Result<String, String> {
    require_session(&state, &token)?;
    Ok(state
        .get_setting("base_dir")
        .unwrap_or_else(|| DEFAULT_BASE_DIR.to_string()))
}

#[tauri::command]
pub fn set_base_dir(
    state: State<'_, Arc<AppState>>,
    token: String,
    base_dir: String,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    let base_dir = base_dir.trim().to_string();
    if base_dir.is_empty() {
        return Err("基地址不能为空".into());
    }
    state.set_setting("base_dir", &base_dir);
    state.add_log(&format!("修改仓库基地址为 {base_dir}"), &username);
    Ok(())
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
    Ok(McpConfig {
        api_key: state
            .get_setting("mcp_api_key")
            .unwrap_or_default(),
        exe_path,
    })
}

#[tauri::command]
pub fn regenerate_mcp_api_key(
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<String, String> {
    let username = require_session(&state, &token)?;
    let key = format!("grs_{}", Uuid::new_v4().simple());
    state.set_setting("mcp_api_key", &key);
    state.add_log("重新生成 MCP APIKEY", &username);
    Ok(key)
}

/// 按时间倒序列出操作日志（软件全部操作入库 sqlite）
#[tauri::command]
pub fn list_operation_logs(
    state: State<'_, Arc<AppState>>,
    token: String,
    limit: Option<i64>,
) -> Result<Vec<OperationLog>, String> {
    require_session(&state, &token)?;
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let conn = lock(&state.conn);
    let mut stmt = conn
        .prepare(
            "SELECT id, action, operator, created_at FROM operation_logs
             ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let logs = stmt
        .query_map(params![limit], |row| {
            Ok(OperationLog {
                id: row.get(0)?,
                action: row.get(1)?,
                operator: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(logs)
}
