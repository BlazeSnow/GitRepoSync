//! MCP stdio 协议层：JSON-RPC 2.0 主循环、鉴权、panic 隔离与诊断日志。
//! 工具定义与分发见 mcp/tools.rs；由 main.rs 的 `mcp` 子命令进入。

mod tools;

use crate::lang::{tr, tr_a, Lang};
use crate::state::AppState;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;
use tools::{tools_call, tools_list};

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
    eprintln!(
        "[mcp] stdio 服务已启动（APIKEY 鉴权{}）",
        if authorized { "通过" } else { "未通过" }
    );
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

/// 返回 None 表示无需应答（通知类消息）。
/// 外层：单请求 panic 隔离（应答内部错误、进程存活）与 stderr 诊断日志——
/// stdio 模式下 stderr 不参与协议，客户端可见，用于排查断连类问题。
fn handle_line(
    state: &Arc<AppState>,
    authorized: bool,
    session_lang: &mut Lang,
    line: &str,
) -> Option<String> {
    // 通知类消息不应答、不计入诊断日志
    let method = serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(str::to_string));
    if method.as_deref().is_some_and(|m| m.starts_with("notifications/")) {
        return handle_line_inner(state, authorized, session_lang, line);
    }
    let started = std::time::Instant::now();
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handle_line_inner(state, authorized, session_lang, line)
    }))
    .unwrap_or_else(|p| {
        let msg = if let Some(s) = p.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = p.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        eprintln!("[mcp] 请求处理 panic：{msg}");
        Some(
            rpc_error(&Value::Null, -32603, &tr(*session_lang, "mcp-internal-error"))
                .to_string(),
        )
    });
    eprintln!(
        "[mcp] {}（{} ms）",
        method.as_deref().unwrap_or("?"),
        started.elapsed().as_millis()
    );
    response
}

fn handle_line_inner(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::state::AppState;

    fn open_state() -> (Arc<AppState>, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("grs-mcp-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        (
            Arc::new(AppState::open(root.join("app.db")).unwrap()),
            root,
        )
    }

    /// 协议层：通知不应答、鉴权拦截、解析错误、initialize 版本与语言切换、未知方法、ping
    #[test]
    fn handle_line_protocol_behaviour() {
        let (state, root) = open_state();
        let mut lang = Lang::Zh;
        // 通知类消息不应答
        assert_eq!(
            handle_line(
                &state,
                true,
                &mut lang,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            ),
            None
        );
        // 鉴权未通过：所有请求被拒
        let resp = handle_line(
            &state,
            false,
            &mut lang,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        )
        .unwrap();
        assert!(resp.contains("-32001"));
        // 解析错误
        let resp = handle_line(&state, true, &mut lang, "not-json").unwrap();
        assert!(resp.contains("-32700"));
        // initialize：返回协议版本，locale 切换会话语言
        let resp = handle_line(
            &state,
            true,
            &mut lang,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"locale":"en-US"}}"#,
        )
        .unwrap();
        assert!(resp.contains("protocolVersion"));
        assert_eq!(lang, Lang::En);
        // 会话语言影响后续消息（method not found 为英文文案）
        let resp = handle_line(
            &state,
            true,
            &mut lang,
            r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#,
        )
        .unwrap();
        assert!(resp.contains("-32601"));
        assert!(resp.contains("method not found"));
        // ping 应答 result
        let resp = handle_line(
            &state,
            true,
            &mut lang,
            r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#,
        )
        .unwrap();
        assert!(resp.contains("\"result\""));
        std::fs::remove_dir_all(&root).ok();
    }
}
