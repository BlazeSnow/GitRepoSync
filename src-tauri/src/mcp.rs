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
pub fn run_stdio(state: Arc<AppState>, provided_key: Option<String>) {
    let authorized = {
        let expected = state.get_setting("mcp_api_key").unwrap_or_default();
        !expected.is_empty()
            && provided_key.is_some()
            && provided_key.as_deref() == Some(expected.as_str())
    };
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
                if let Some(response) = handle_line(&state, authorized, line) {
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
fn handle_line(state: &Arc<AppState>, authorized: bool, line: &str) -> Option<String> {
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
        return Some(rpc_error(&id, -32001, "unauthorized: APIKEY 不正确").to_string());
    }

    let params = v.get("params").cloned().unwrap_or(json!({}));
    let result = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "git-repo-sync", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(state, &params),
        other => Err((-32601, format!("method not found: {other}"))),
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

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "list_repos",
                "description": "列出所有已配置的同步仓库及其最近一次同步状态",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "add_repo",
                "description": "新增一个同步仓库：从源仓库拉取到本地基地址作为中转站（更新 LFS 与 submodule），再推送到目标仓库地址",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "仓库名称（同时是本地基地址下的目录名）" },
                        "source": { "type": "string", "description": "源地址（Git 仓库 URL）" },
                        "target": { "type": "string", "description": "目标仓库地址（Git 仓库 URL）" }
                    },
                    "required": ["name", "source", "target"]
                }
            },
            {
                "name": "remove_repo",
                "description": "删除指定的同步仓库",
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": { "type": "string", "description": "仓库 ID" } },
                    "required": ["id"]
                }
            },
            {
                "name": "sync_repo",
                "description": "立即开始同步指定仓库（异步执行，可用 get_sync_status 查询进度）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": { "type": "string", "description": "仓库 ID" } },
                    "required": ["id"]
                }
            },
            {
                "name": "get_sync_status",
                "description": "查询所有仓库的最近同步状态",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_base_dir",
                "description": "查询本地仓库基地址（中转站目录）",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "set_base_dir",
                "description": "修改本地仓库基地址（中转站目录）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "base_dir": { "type": "string", "description": "基地址路径，支持 ~ 开头" } },
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

fn tools_call(state: &Arc<AppState>, params: &Value) -> Result<Value, (i64, String)> {
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
                return Err((-32602, "name / source / target 不能为空".into()));
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
            state.add_log(&format!("通过 MCP 添加仓库「{rname}」"), OPERATOR);
            serde_json::to_value(repo).map_err(|e| e.to_string())
        }
        "remove_repo" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if lock(&state.syncing).contains(&id) {
                return Err((-32602, "该仓库正在同步，请先停止".into()));
            }
            let deleted = {
                let conn = lock(&state.conn);
                conn.execute("DELETE FROM repos WHERE id = ?1", params![id])
                    .map_err(|e| (-32602, e.to_string()))?
            };
            if deleted == 0 {
                return Err((-32602, "仓库不存在".into()));
            }
            state.add_log("通过 MCP 删除仓库", OPERATOR);
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
                return Err((-32602, "仓库不存在".into()));
            }
            let started = repos::spawn_sync(state.clone(), None, id, OPERATOR.to_string());
            state.add_log("通过 MCP 触发同步", OPERATOR);
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
                return Err((-32602, "base_dir 不能为空".into()));
            }
            state.set_setting("base_dir", &base_dir);
            state.add_log(&format!("通过 MCP 修改仓库基地址为 {base_dir}"), OPERATOR);
            Ok(json!({ "base_dir": base_dir }))
        }
        _ => return Err((-32602, format!("unknown tool: {name}"))),
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
