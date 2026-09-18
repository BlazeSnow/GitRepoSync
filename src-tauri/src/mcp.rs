use crate::repos;
use crate::state::{lock, AppState, Repo};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;
use uuid::Uuid;

/// MCP stdio 主循环：逐行读取 stdin 的 JSON-RPC 2.0 消息并应答（通知类消息不应答）。
/// 鉴权：客户端须通过环境变量 GIT_REPO_SYNC_API_KEY 或 --api-key 参数提供 APIKEY，
/// 与设置页生成的 APIKEY 一致方可访问；不一致时所有请求均被拒绝。
pub fn run_stdio(state: Arc<AppState>, provided_key: Option<String>) {
    let authorized = {
        let expected = lock(&state.settings).mcp_api_key.clone();
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
                "description": "新增一个同步仓库（以镜像方式从源地址同步到目标地址）",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "仓库名称" },
                        "source": { "type": "string", "description": "源地址（Git 仓库 URL）" },
                        "target": { "type": "string", "description": "目标地址（本地保存路径）" }
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
            }
        ]
    })
}

fn tools_call(state: &Arc<AppState>, params: &Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result: Result<Value, String> = match name {
        "list_repos" | "get_sync_status" => {
            let repos = lock(&state.repos);
            serde_json::to_value(&*repos).map_err(|e| e.to_string())
        }
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
                name: rname,
                source,
                target,
                last_synced: None,
                last_status: "idle".into(),
                last_message: None,
            };
            lock(&state.repos).push(repo.clone());
            state.save_repos().map_err(|e| (-32602, e))?;
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
            lock(&state.repos).retain(|r| r.id != id);
            state.save_repos().map_err(|e| (-32602, e))?;
            Ok(json!({ "deleted": true }))
        }
        "sync_repo" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let exists = lock(&state.repos).iter().any(|r| r.id == id);
            if !exists {
                return Err((-32602, "仓库不存在".into()));
            }
            let started = repos::spawn_sync(state.clone(), None, id);
            Ok(json!({ "started": started }))
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
