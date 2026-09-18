use crate::repos;
use crate::state::{lock, AppState, Repo};
use serde_json::{json, Value};
use std::io::Cursor;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Method, Response, Server};
use uuid::Uuid;

type Resp = Response<Cursor<Vec<u8>>>;

/// 在独立线程中启动 MCP 服务（Streamable HTTP / JSON-RPC 2.0，APIKEY 鉴权）
pub fn start_server(state: Arc<AppState>, port: u16) {
    thread::spawn(move || {
        let server = match Server::http(("127.0.0.1", port)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("MCP 服务启动失败（端口 {port}）：{e}");
                return;
            }
        };
        println!("MCP 服务已启动：http://127.0.0.1:{port}/mcp");
        loop {
            let mut request = match server.recv() {
                Ok(r) => r,
                Err(_) => break,
            };
            let response = handle_request(&state, &mut request);
            let _ = request.respond(response);
        }
    });
}

fn respond_json(status: u16, body: &Value) -> Resp {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header");
    Response::from_string(serde_json::to_string(body).unwrap_or_default())
        .with_header(header)
        .with_status_code(status)
}

fn respond_empty(status: u16) -> Resp {
    Response::from_string(String::new()).with_status_code(status)
}

fn handle_request(state: &Arc<AppState>, request: &mut tiny_http::Request) -> Resp {
    if request.method() != &Method::Post
        || !request.url().split('?').next().unwrap_or("").starts_with("/mcp")
    {
        return respond_json(404, &json!({ "error": "not found" }));
    }

    // APIKEY 鉴权：Authorization: Bearer <key> 或 X-Api-Key: <key>
    let expected = lock(&state.settings).mcp_api_key.clone();
    let provided = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().to_string())
        .and_then(|h| h.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| {
            request
                .headers()
                .iter()
                .find(|h| h.field.equiv("X-Api-Key"))
                .map(|h| h.value.as_str().to_string())
        });
    match provided {
        Some(k) if !expected.is_empty() && k == expected => {}
        _ => return respond_json(401, &json!({ "error": "unauthorized" })),
    }

    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return respond_json(400, &json!({ "error": "invalid body" }));
    }
    let v: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            return respond_json(200, &json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } }))
        }
    };
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // 通知类请求无响应体
    if method.starts_with("notifications/") {
        return respond_empty(202);
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

    match result {
        Ok(r) => respond_json(200, &json!({ "jsonrpc": "2.0", "id": id, "result": r })),
        Err((code, msg)) => respond_json(
            200,
            &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } }),
        ),
    }
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
