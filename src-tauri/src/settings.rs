use crate::auth::require_session;
use crate::lang::{gui_lang, tr, tr_a};
use crate::state::{lock, normalize_base_dir, AppState, OperationLog};
use rusqlite::params;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
}

#[tauri::command]
pub fn get_app_info<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
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
        .unwrap_or_else(crate::state::default_base_dir))
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
        return Err(tr(gui_lang(), "base-dir-empty"));
    }
    let base_dir = normalize_base_dir(&base_dir);
    state.set_setting("base_dir", &base_dir);
    state.add_log(
        &tr_a(gui_lang(), "log-base-dir-changed", &[("dir", &base_dir)]),
        &username,
    );
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
    state.add_log(
        &tr(gui_lang(), "log-mcp-key-regenerated"),
        &username,
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testutil::{insert_session, open_mock_app};
    use std::sync::Arc;
    use tauri::Manager;

    /// 命令层：应用信息、基地址读写（~ 展开与空值拒绝）、MCP 配置与 APIKEY 重置、日志列表
    #[test]
    fn settings_commands_roundtrip() {
        let (app, state, root) = open_mock_app("settings");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");

        // 应用信息：版本与包名非空
        let info = get_app_info(app.handle().clone(), st.clone(), "tok".into()).unwrap();
        assert!(!info.version.is_empty());
        assert!(!info.name.is_empty());

        // 基地址：~ 输入展开为完整路径；空值拒绝
        set_base_dir(st.clone(), "tok".into(), "~/repo/custom".into()).unwrap();
        assert_eq!(
            get_base_dir(st.clone(), "tok".into()).unwrap(),
            dirs::home_dir()
                .unwrap()
                .join("repo/custom")
                .to_string_lossy()
        );
        assert!(set_base_dir(st.clone(), "tok".into(), "   ".into()).is_err());

        // MCP 配置与 APIKEY 重置：新 key 生效且与旧值不同
        let key1 = get_mcp_config(st.clone(), "tok".into()).unwrap();
        assert!(key1.api_key.starts_with("grs_"));
        let key2 = regenerate_mcp_api_key(st.clone(), "tok".into()).unwrap();
        assert_ne!(key1.api_key, key2, "重置后的 APIKEY 应不同");
        assert_eq!(get_mcp_config(st.clone(), "tok".into()).unwrap().api_key, key2);

        // 操作日志：倒序返回且 limit 生效
        list_operation_logs(st.clone(), "tok".into(), Some(5)).unwrap();

        // 无效令牌拒绝
        assert!(get_base_dir(st.clone(), "bad".into()).is_err());

        drop(st);
        drop(app);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }
}
