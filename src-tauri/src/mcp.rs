use crate::lang::{tr, tr_a, Lang};
use crate::repos;
use crate::state::{lock, AppState, Repo};
use rusqlite::params;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;
use uuid::Uuid;

const OPERATOR: &str = "mcp";

/// MCP stdio 主循环：逐行读取 stdin 的 JSON-RPC 2.0 消息并应答（通知类消息不应答）。
/// 鉴权：客户端须通过环境变量 GIT_REPO_SYNC_API_KEY 或 --api-key 参数提供 APIKEY，
/// 与设置页生成的 APIKEY 一致方可访问；不一致时所有请求均被拒绝。
/// 会话语言：initialize 请求的 locale 字段（可被 GIT_REPO_SYNC_LANG 覆盖），缺省中文。
pub fn run_stdio(state: Arc<AppState>, provided_key: Option<String>) {
    let authorized = {
        let expected = state.get_setting("mcp_api_key").unwrap_or_default();
        !expected.is_empty()
            && provided_key.is_some()
            && provided_key.as_deref() == Some(expected.as_str())
    };
    let mut session_lang = Lang::from_env();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    loop {
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(response) = handle_line(&state, authorized, &mut session_lang, line) {
                    let _ = writeln!(out, "{response}");
                    let _ = out.flush();
                }
            }
            Err(e) => {
                eprintln!("stdin 读取失败: {e}");
                break;
            }
        }
    }
}

/// 返回 None 表示无需应答（通知类消息）
fn handle_line(
    state: &Arc<AppState>,
    authorized: bool,
    session_lang: &mut Lang,
    line: &str,
) -> Option<String> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } })
                    .to_string(),
            )
        }
    };
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // 通知类消息不应答
    if method.starts_with("notifications/") {
        return None;
    }

    // APIKEY 鉴权：启动时校验，未通过时所有请求均拒绝
    if !authorized {
        return Some(rpc_error(&id, -32001, &tr(*session_lang, "mcp-unauthorized")).to_string());
    }

    let params = v.get("params").cloned().unwrap_or(json!({}));
    let result = match method.as_str() {
        "initialize" => {
            // 客户端 locale 决定本次会话的后续消息语言
            if let Some(locale) = params.get("locale").and_then(|l| l.as_str()) {
                if let Some(l) = Lang::parse_tag(Some(locale)) {
                    *session_lang = l;
                }
            }
            Ok(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "git-repo-sync", "version": env!("CARGO_PKG_VERSION") }
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list(*session_lang)),
        "tools/call" => tools_call(state, *session_lang, &params),
        other => Err((
            -32601,
            tr_a(*session_lang, "mcp-method-not-found", &[("method", other)]),
        )),
    };

    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }).to_string(),
        Err((code, msg)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } }).to_string()
        }
    })
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// 工具描述按会话语言返回；工具名称为协议契约，不翻译
fn tools_list(lang: Lang) -> Value {
    json!({
        "tools": [
            {
                "name": "list_repos",
                "description": tr(lang, "tool-list-repos"),
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "add_repo",
                "description": tr(lang, "tool-add-repo"),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": tr(lang, "tool-add-repo-name") },
                        "source": { "type": "string", "description": tr(lang, "tool-add-repo-source") },
                        "target": { "type": "string", "description": tr(lang, "tool-add-repo-target") }
                    },
                    "required": ["name", "source", "target"]
                }
            },
            {
                "name": "remove_repo",
                "description": tr(lang, "tool-remove-repo"),
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": { "type": "string", "description": tr(lang, "tool-remove-repo-id") } },
                    "required": ["id"]
                }
            },
            {
                "name": "sync_repo",
                "description": tr(lang, "tool-sync-repo"),
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": { "type": "string", "description": tr(lang, "tool-sync-repo-id") } },
                    "required": ["id"]
                }
            },
            {
                "name": "get_sync_status",
                "description": tr(lang, "tool-get-sync-status"),
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_base_dir",
                "description": tr(lang, "tool-get-base-dir"),
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "set_base_dir",
                "description": tr(lang, "tool-set-base-dir"),
                "inputSchema": {
                    "type": "object",
                    "properties": { "base_dir": { "type": "string", "description": tr(lang, "tool-set-base-dir-base-dir") } },
                    "required": ["base_dir"]
                }
            }
        ]
    })
}

fn repo_from_row(row: &rusqlite::Row) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        name: row.get(1)?,
        source: row.get(2)?,
        target: row.get(3)?,
        last_synced: row.get(4)?,
        last_status: row.get(5)?,
        last_message: row.get(6)?,
    })
}

const REPO_COLS: &str = "id, name, source, target, last_synced, last_status, last_message";

fn list_repos_value(state: &AppState) -> Result<Value, String> {
    let conn = lock(&state.conn);
    let mut stmt = conn
        .prepare(&format!("SELECT {REPO_COLS} FROM repos ORDER BY name"))
        .map_err(|e| e.to_string())?;
    let repos = stmt
        .query_map([], repo_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    serde_json::to_value(repos).map_err(|e| e.to_string())
}

fn tools_call(state: &Arc<AppState>, lang: Lang, params: &Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result: Result<Value, String> = match name {
        "list_repos" | "get_sync_status" => list_repos_value(state),
        "add_repo" => {
            let field = |k: &str| {
                args.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            let (rname, source, target) = (field("name"), field("source"), field("target"));
            if rname.is_empty() || source.is_empty() || target.is_empty() {
                return Err((-32602, tr(lang, "repo-fields-empty")));
            }
            let repo = Repo {
                id: Uuid::new_v4().to_string(),
                name: rname.clone(),
                source,
                target,
                last_synced: None,
                last_status: "idle".into(),
                last_message: None,
            };
            {
                let conn = lock(&state.conn);
                conn.execute(
                    "INSERT INTO repos (id, name, source, target, last_synced, last_status, last_message)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        repo.id,
                        repo.name,
                        repo.source,
                        repo.target,
                        repo.last_synced,
                        repo.last_status,
                        repo.last_message
                    ],
                )
                .map_err(|e| (-32602, e.to_string()))?;
            }
            state.add_log(
                &tr_a(lang, "log-mcp-repo-added", &[("name", &rname)]),
                OPERATOR,
            );
            serde_json::to_value(repo).map_err(|e| e.to_string())
        }
        "remove_repo" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if lock(&state.syncing).contains(&id) {
                return Err((-32602, tr(lang, "repo-syncing")));
            }
            let deleted = {
                let conn = lock(&state.conn);
                conn.execute("DELETE FROM repos WHERE id = ?1", params![id])
                    .map_err(|e| (-32602, e.to_string()))?
            };
            if deleted == 0 {
                return Err((-32602, tr(lang, "repo-not-found")));
            }
            state.add_log(&tr(lang, "log-mcp-repo-deleted"), OPERATOR);
            Ok(json!({ "deleted": true }))
        }
        "sync_repo" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let exists = {
                let conn = lock(&state.conn);
                conn.query_row(
                    "SELECT COUNT(*) FROM repos WHERE id = ?1",
                    params![id],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(|e| (-32602, e.to_string()))?
                    > 0
            };
            if !exists {
                return Err((-32602, tr(lang, "repo-not-found")));
            }
            let started = repos::spawn_sync(state.clone(), None, id, OPERATOR.to_string(), lang);
            state.add_log(&tr(lang, "log-mcp-sync"), OPERATOR);
            Ok(json!({ "started": started }))
        }
        "get_base_dir" => Ok(json!({ "base_dir": state
            .get_setting("base_dir")
            .unwrap_or_else(|| crate::state::DEFAULT_BASE_DIR.to_string()) })),
        "set_base_dir" => {
            let base_dir = args
                .get("base_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if base_dir.is_empty() {
                return Err((-32602, tr(lang, "base-dir-empty")));
            }
            state.set_setting("base_dir", &base_dir);
            state.add_log(
                &tr_a(lang, "log-mcp-base-dir-changed", &[("dir", &base_dir)]),
                OPERATOR,
            );
            Ok(json!({ "base_dir": base_dir }))
        }
        _ => {
            return Err((
                -32602,
                tr_a(lang, "mcp-unknown-tool", &[("tool", name)]),
            ))
        }
    };

    match result {
        Ok(v) => Ok(json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() } ],
            "isError": false
        })),
        Err(e) => Ok(json!({
            "content": [ { "type": "text", "text": e } ],
            "isError": true
        })),
    }
}
