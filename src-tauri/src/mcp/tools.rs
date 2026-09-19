//! MCP 工具层：tools/list 的工具清单（名称为协议契约，描述按会话语言返回）
//! 与 tools/call 的分发实现。仓库/日志读写直接走 AppState 的 SQLite，
//! 同步触发复用 crate::sync 的队列；发现复用 crate::discover。

use crate::discover;
use crate::lang::{tr, tr_a, Lang};
use crate::state::{lock, normalize_base_dir, repo_from_row, AppState, OperationLog, REPO_COLS};
use crate::sync;
use rusqlite::params;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

const OPERATOR: &str = "mcp";

/// 工具描述按会话语言返回；工具名称为协议契约，不翻译
pub(super) fn tools_list(lang: Lang) -> Value {
    json!({
        "tools": [
            {
                "name": "list_repos",
                "description": tr(lang, "tool-list-repos"),
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "discover_repos",
                "description": tr(lang, "tool-discover-repos"),
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
                        "target": { "type": "string", "description": tr(lang, "tool-add-repo-target") },
                        "targets": {
                            "type": "array",
                            "description": tr(lang, "tool-add-repo-targets"),
                            "items": {
                                "type": "object",
                                "properties": {
                                    "remote": { "type": "string" },
                                    "url": { "type": "string" }
                                }
                            }
                        }
                    },
                    "required": ["name", "source", "target"]
                }
            },
            {
                "name": "update_repo",
                "description": tr(lang, "tool-update-repo"),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": tr(lang, "tool-update-repo-id") },
                        "name": { "type": "string", "description": tr(lang, "tool-update-repo-name") },
                        "source": { "type": "string", "description": tr(lang, "tool-update-repo-source") },
                        "targets": {
                            "type": "array",
                            "description": tr(lang, "tool-update-repo-targets"),
                            "items": {
                                "type": "object",
                                "properties": {
                                    "remote": { "type": "string" },
                                    "url": { "type": "string" }
                                }
                            }
                        }
                    },
                    "required": ["id"]
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
                "name": "sync_repos",
                "description": tr(lang, "tool-sync-repos"),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": tr(lang, "tool-sync-repos-ids")
                        },
                        "days": {
                            "type": "integer",
                            "description": tr(lang, "tool-sync-repos-days")
                        }
                    }
                }
            },
            {
                "name": "get_sync_status",
                "description": tr(lang, "tool-get-sync-status"),
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_logs",
                "description": tr(lang, "tool-list-logs"),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": tr(lang, "tool-list-logs-limit") }
                    }
                }
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

fn list_repos_value(state: &AppState) -> Result<Value, String> {
    let conn = lock(&state.conn);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {REPO_COLS} FROM repos WHERE hidden = 0 ORDER BY name"
        ))
        .map_err(|e| e.to_string())?;
    let mut repos = stmt
        .query_map([], repo_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    crate::state::attach_targets(&conn, &mut repos);
    serde_json::to_value(repos).map_err(|e| e.to_string())
}

/// 按时间倒序列出操作日志（limit 已由调用方钳制）
fn list_logs_value(state: &AppState, limit: i64) -> Result<Value, String> {
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
    serde_json::to_value(logs).map_err(|e| e.to_string())
}

/// 按 id 读取完整仓库（含目标状态）并序列化；供 add / update 工具响应复用
fn repo_value_by_id(state: &AppState, id: &str, lang: Lang) -> Result<Value, String> {
    let conn = lock(&state.conn);
    let mut repo = conn
        .query_row(
            &format!("SELECT {REPO_COLS} FROM repos WHERE id = ?1"),
            params![id],
            repo_from_row,
        )
        .map_err(|_| tr(lang, "repo-not-found"))?;
    crate::state::attach_targets(&conn, std::slice::from_mut(&mut repo));
    serde_json::to_value(repo).map_err(|e| e.to_string())
}

pub(super) fn tools_call(
    state: &Arc<AppState>,
    lang: Lang,
    params: &Value,
) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result: Result<Value, String> = match name {
        "list_repos" | "get_sync_status" => list_repos_value(state),
        "discover_repos" => {
            // 扫描基地址（中转站目录）登记新仓库：与界面加载逻辑一致，
            // 新登记的仓库以 mcp 为操作人写入日志；返回登记后的完整列表
            discover::discover(state, OPERATOR, lang).and_then(|()| list_repos_value(state))
        }
        "list_logs" => {
            // 只读操作不写日志（与 list_repos 一致），避免读取行为自我刷屏
            let limit = args
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(200)
                .clamp(1, 1000);
            list_logs_value(state, limit)
        }
        "add_repo" => {
            let field = |k: &str| {
                args.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            let (rname, source) = (field("name"), field("source"));
            if rname.is_empty() || source.is_empty() {
                return Err((-32602, tr(lang, "repo-fields-empty")));
            }
            // 目标：优先 targets 数组 [{remote,url}]；兼容单 target 字符串（远端名 backup）
            let mut targets: Vec<(String, String)> = args
                .get("targets")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| {
                            let remote = t.get("remote")?.as_str()?.trim().to_string();
                            let url = t.get("url")?.as_str()?.trim().to_string();
                            (!remote.is_empty() && !url.is_empty()).then_some((remote, url))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if targets.is_empty() {
                if let Some(t) = args.get("target").and_then(|v| v.as_str()) {
                    let t = t.trim();
                    if !t.is_empty() {
                        targets.push(("backup".into(), t.to_string()));
                    }
                }
            }
            if targets.is_empty() {
                return Err((-32602, tr(lang, "sync-no-targets")));
            }
            // 幂等：同名仓库已存在（含隐藏的）时更新源地址、合并目标并重新登记，
            // 不再创建重复条目——name 同时是中转目录名与自动发现的身份
            let existing: Option<String> = {
                let conn = lock(&state.conn);
                conn.query_row(
                    "SELECT id FROM repos WHERE name = ?1",
                    params![rname],
                    |r| r.get::<_, String>(0),
                )
                .ok()
            };
            let created = existing.is_none();
            let repo_id = match existing {
                Some(id) => {
                    let conn = lock(&state.conn);
                    conn.execute(
                        "UPDATE repos SET source = ?1, hidden = 0 WHERE id = ?2",
                        params![source, id],
                    )
                    .map_err(|e| (-32602, e.to_string()))?;
                    for (remote, url) in &targets {
                        conn.execute(
                            "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)
                             ON CONFLICT(repo_id, remote) DO UPDATE SET url = excluded.url",
                            params![id, remote, url],
                        )
                        .map_err(|e| (-32602, e.to_string()))?;
                    }
                    id
                }
                None => {
                    let id = Uuid::new_v4().to_string();
                    let conn = lock(&state.conn);
                    conn.execute(
                        "INSERT INTO repos (id, name, source, target, last_synced, last_status, last_message)
                         VALUES (?1, ?2, ?3, '', ?4, ?5, ?6)",
                        params![id, rname, source, None::<i64>, "idle", None::<String>],
                    )
                    .map_err(|e| (-32602, e.to_string()))?;
                    for (remote, url) in &targets {
                        conn.execute(
                            "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)",
                            params![id, remote, url],
                        )
                        .map_err(|e| (-32602, e.to_string()))?;
                    }
                    id
                }
            };
            state.add_log(
                &tr_a(lang, "log-mcp-repo-added", &[("name", &rname)]),
                OPERATOR,
            );
            let mut value = repo_value_by_id(state, &repo_id, lang).map_err(|e| (-32602, e))?;
            if let Some(obj) = value.as_object_mut() {
                obj.insert("created".into(), Value::Bool(created));
            }
            Ok(value)
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
            // 存在性按 repos 行本身判定：自动发现的仓库可能没有目标，
            // sync_targets 的删除行数不能作为判定（曾把“无目标仓库”误报为不存在）。
            // 移除 = 软删除：隐藏并清空目标；基地址内目录不删除、不会被自动发现重新登记，
            // 与界面删除语义一致
            let name = {
                let conn = lock(&state.conn);
                let name: String = conn
                    .query_row("SELECT name FROM repos WHERE id = ?1", params![id], |r| r.get(0))
                    .map_err(|_| (-32602, tr(lang, "repo-not-found")))?;
                conn.execute("UPDATE repos SET hidden = 1 WHERE id = ?1", params![id])
                    .map_err(|e| (-32602, e.to_string()))?;
                conn.execute("DELETE FROM sync_targets WHERE repo_id = ?1", params![id])
                    .map_err(|e| (-32602, e.to_string()))?;
                name
            };
            state.add_log(
                &tr_a(lang, "log-mcp-repo-deleted", &[("name", &name)]),
                OPERATOR,
            );
            Ok(json!({ "deleted": true, "id": id, "name": name }))
        }
        "update_repo" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if lock(&state.syncing).contains(&id) {
                return Err((-32602, tr(lang, "repo-syncing")));
            }
            let optional_field = |k: &str| {
                args.get(k)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            let name = optional_field("name");
            let source = optional_field("source");
            // targets 提供即整体替换（与界面编辑一致）；仅补充目标请用 add_repo（合并语义）
            let targets: Option<Vec<(String, String)>> = args
                .get("targets")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| {
                            let remote = t.get("remote")?.as_str()?.trim().to_string();
                            let url = t.get("url")?.as_str()?.trim().to_string();
                            (!remote.is_empty() && !url.is_empty()).then_some((remote, url))
                        })
                        .collect()
                });
            if let Some(ts) = &targets {
                if ts.is_empty() {
                    return Err((-32602, tr(lang, "sync-no-targets")));
                }
            }
            if name.is_none() && source.is_none() && targets.is_none() {
                return Err((-32602, tr(lang, "update-repo-no-fields")));
            }
            let log_name = {
                let conn = lock(&state.conn);
                let current_name: String = conn
                    .query_row("SELECT name FROM repos WHERE id = ?1", params![id], |r| {
                        r.get(0)
                    })
                    .map_err(|_| (-32602, tr(lang, "repo-not-found")))?;
                // 改名禁止与现有名称冲突（name 唯一，且是自动发现的身份）
                if let Some(n) = &name {
                    let dup: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM repos WHERE name = ?1 AND id != ?2",
                            params![n, id],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    if dup > 0 {
                        return Err((-32602, tr_a(lang, "repo-name-exists", &[("name", n)])));
                    }
                    conn.execute("UPDATE repos SET name = ?1 WHERE id = ?2", params![n, id])
                        .map_err(|e| (-32602, e.to_string()))?;
                }
                if let Some(s) = &source {
                    conn.execute("UPDATE repos SET source = ?1 WHERE id = ?2", params![s, id])
                        .map_err(|e| (-32602, e.to_string()))?;
                }
                if let Some(ts) = &targets {
                    conn.execute("DELETE FROM sync_targets WHERE repo_id = ?1", params![id])
                        .map_err(|e| (-32602, e.to_string()))?;
                    for (remote, url) in ts {
                        conn.execute(
                            "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)",
                            params![id, remote, url],
                        )
                        .map_err(|e| (-32602, e.to_string()))?;
                    }
                }
                name.unwrap_or(current_name)
            };
            state.add_log(
                &tr_a(lang, "log-mcp-repo-updated", &[("name", &log_name)]),
                OPERATOR,
            );
            repo_value_by_id(state, &id, lang)
        }
        "sync_repo" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let (exists, source, target_n) = {
                let conn = lock(&state.conn);
                let source: String = conn
                    .query_row(
                        "SELECT source FROM repos WHERE id = ?1",
                        params![id],
                        |r| r.get(0),
                    )
                    .unwrap_or_default();
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sync_targets WHERE repo_id = ?1",
                        params![id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let exists: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM repos WHERE id = ?1",
                        params![id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                (exists > 0, source, n)
            };
            if !exists {
                return Err((-32602, tr(lang, "repo-not-found")));
            }
            if source.is_empty() {
                return Err((-32602, tr(lang, "sync-no-source")));
            }
            if target_n == 0 {
                return Err((-32602, tr(lang, "sync-no-targets")));
            }
            let started = sync::spawn_sync(state.clone(), None, id, OPERATOR.to_string(), lang);
            state.add_log(&tr(lang, "log-mcp-sync"), OPERATOR);
            Ok(json!({ "started": started }))
        }
        "sync_repos" => {
            // 选择器二选一：ids 显式列表优先；否则 days 范围（0=全部，N=最近 N 天未同步，
            // 与界面范围下拉语义一致）。两者皆缺拒绝，避免误触发全量同步
            let ids_opt: Option<Vec<String>> = args
                .get("ids")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                });
            let days_opt = args.get("days").and_then(|v| v.as_i64());
            let ids = match (ids_opt, days_opt) {
                (Some(ids), _) if !ids.is_empty() => ids,
                (Some(_), None) => {
                    return Err((-32602, tr(lang, "sync-ids-empty")));
                }
                (_, Some(days)) => {
                    sync::select_stale_ids(state, days).map_err(|e| (-32602, e))?
                }
                (None, None) => {
                    return Err((-32602, tr(lang, "sync-repos-no-selector")));
                }
            };
            // 未配置 / 已在同步的仓库报告未启动；真实同步由串行队列执行
            let results = sync::enqueue_syncs(state, None, &ids, OPERATOR, lang);
            let started = results.iter().filter(|(_, s)| *s).count();
            if started > 0 {
                state.add_log(
                    &tr_a(
                        lang,
                        "log-mcp-sync-batch",
                        &[("count", &started.to_string())],
                    ),
                    OPERATOR,
                );
            }
            Ok(json!({
                "requested": ids.len(),
                "started": started,
                "results": results
                    .iter()
                    .map(|(id, s)| json!({ "id": id, "started": s }))
                    .collect::<Vec<_>>(),
            }))
        }
        "get_base_dir" => Ok(json!({ "base_dir": state
            .get_setting("base_dir")
            .unwrap_or_else(crate::state::default_base_dir) })),
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
            let base_dir = normalize_base_dir(&base_dir);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::state::AppState;

    fn open_state() -> (Arc<AppState>, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("grs-mcp-test-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        (
            Arc::new(AppState::open(root.join("app.db")).unwrap()),
            root,
        )
    }

    fn call(state: &Arc<AppState>, tool: &str, args: Value) -> Result<Value, (i64, String)> {
        // tools_call 的 Ok 是 { content: [{text}], isError } 信封：解开为原始值 / Err
        let envelope = tools_call(state, Lang::Zh, &json!({ "name": tool, "arguments": args }))?;
        let is_error = envelope["isError"].as_bool().unwrap_or(false);
        let text = envelope["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if is_error {
            return Err((-32602, text));
        }
        serde_json::from_str(&text).map_err(|e| (-32603, e.to_string()))
    }

    fn count(state: &AppState, sql: &str, id: &str) -> i64 {
        let conn = lock(&state.conn);
        conn.query_row(sql, params![id], |r| r.get(0)).unwrap()
    }

    /// 同名 add_repo 幂等：更新源地址、按 remote 合并目标，不产生重复条目
    #[test]
    fn add_repo_is_idempotent_by_name() {
        let (state, root) = open_state();
        let first = call(
            &state,
            "add_repo",
            json!({
                "name": "demo", "source": "https://src/demo.git",
                "targets": [{ "remote": "backup", "url": "https://bak/demo.git" }]
            }),
        )
        .unwrap();
        assert_eq!(first["created"], json!(true));
        let second = call(
            &state,
            "add_repo",
            json!({
                "name": "demo", "source": "https://src2/demo.git",
                "targets": [{ "remote": "gitlab", "url": "https://gl/demo.git" }]
            }),
        )
        .unwrap();
        assert_eq!(second["created"], json!(false));
        assert_eq!(second["id"], first["id"], "同名 add_repo 返回同一条目");
        assert_eq!(second["source"], json!("https://src2/demo.git"));
        assert_eq!(
            count(&state, "SELECT COUNT(*) FROM repos WHERE name = ?1", "demo"),
            1
        );
        let id = second["id"].as_str().unwrap();
        assert_eq!(
            count(&state, "SELECT COUNT(*) FROM sync_targets WHERE repo_id = ?1", id),
            2,
            "目标按 remote 合并：backup + gitlab"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// 无目标仓库（自动发现形态）删除必须成功：曾按 sync_targets 删除行数判定而误报不存在
    #[test]
    fn remove_repo_succeeds_without_targets() {
        let (state, root) = open_state();
        let added = call(
            &state,
            "add_repo",
            json!({
                "name": "solo", "source": "https://src/solo.git",
                "targets": [{ "remote": "backup", "url": "https://bak/solo.git" }]
            }),
        )
        .unwrap();
        let id = added["id"].as_str().unwrap().to_string();
        {
            let conn = lock(&state.conn);
            conn.execute("DELETE FROM sync_targets WHERE repo_id = ?1", params![id])
                .unwrap();
        }
        let removed = call(&state, "remove_repo", json!({ "id": id })).unwrap();
        assert_eq!(removed["deleted"], json!(true));
        assert_eq!(removed["name"], json!("solo"));
        let hidden: i64 = {
            let conn = lock(&state.conn);
            conn.query_row("SELECT hidden FROM repos WHERE id = ?1", params![removed["id"].as_str().unwrap()], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(hidden, 1, "移除为软删除（隐藏）");
        // 不存在的 id 报“仓库不存在”
        assert_eq!(
            call(&state, "remove_repo", json!({ "id": "nope" })).unwrap_err().0,
            -32602
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// update_repo：targets 提供即整体替换，支持改名；空 targets 与空参数拒绝
    #[test]
    fn update_repo_replaces_targets_and_renames() {
        let (state, root) = open_state();
        let added = call(
            &state,
            "add_repo",
            json!({
                "name": "old", "source": "https://src/old.git",
                "targets": [
                    { "remote": "a", "url": "https://a/old.git" },
                    { "remote": "b", "url": "https://b/old.git" }
                ]
            }),
        )
        .unwrap();
        let id = added["id"].as_str().unwrap();
        let updated = call(
            &state,
            "update_repo",
            json!({
                "id": id, "name": "new",
                "targets": [{ "remote": "c", "url": "https://c/new.git" }]
            }),
        )
        .unwrap();
        assert_eq!(updated["name"], json!("new"));
        let targets = updated["targets"].as_array().unwrap();
        assert_eq!(targets.len(), 1, "targets 提供即整体替换");
        assert_eq!(targets[0]["remote"], json!("c"));
        assert!(call(&state, "update_repo", json!({ "id": id, "targets": [] })).is_err());
        assert!(call(&state, "update_repo", json!({ "id": id })).is_err());
        assert_eq!(
            call(&state, "update_repo", json!({ "id": "nope", "source": "s" }))
                .unwrap_err()
                .0,
            -32602
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// list_logs：按时间倒序返回操作日志，limit 生效；只读操作自身不写入日志
    #[test]
    fn list_logs_returns_recent_entries() {
        let (state, root) = open_state();
        for name in ["l1", "l2"] {
            call(
                &state,
                "add_repo",
                json!({
                    "name": name, "source": "https://src/x.git",
                    "targets": [{ "remote": "b", "url": "https://b/x.git" }]
                }),
            )
            .unwrap();
        }
        let logs = call(&state, "list_logs", json!({ "limit": 1 })).unwrap();
        let arr = logs.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["operator"], json!("mcp"));
        assert_eq!(arr[0]["action"].as_str().unwrap().contains("l2"), true, "倒序：最新操作在前");
        let all = call(&state, "list_logs", json!({})).unwrap();
        assert!(all.as_array().unwrap().len() >= 2);
        // limit 钳制到 [1, 1000]，非法值不报错
        let clamped = call(&state, "list_logs", json!({ "limit": 0 })).unwrap();
        assert!(!clamped.as_array().unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// 工具参数校验与 base_dir 往返（~ 展开为完整路径）
    #[test]
    fn mcp_tools_validation_and_base_dir_roundtrip() {
        let (state, root) = open_state();
        // 参数校验：缺源地址 / 无目标 / 未知工具均拒绝
        assert_eq!(
            call(&state, "add_repo", json!({ "name": "a", "source": "" })).unwrap_err().0,
            -32602
        );
        assert!(
            call(&state, "add_repo", json!({ "name": "a", "source": "s" })).is_err(),
            "没有任何目标时拒绝"
        );
        assert!(call(&state, "nope", json!({})).is_err(), "未知工具");
        // base_dir 往返：~ 输入展开为完整路径
        call(&state, "set_base_dir", json!({ "base_dir": "~/repo" })).unwrap();
        let got = call(&state, "get_base_dir", json!({})).unwrap();
        let home = dirs::home_dir().expect("测试环境无用户目录");
        assert_eq!(got["base_dir"], json!(home.join("repo").to_string_lossy()));
        // 空值拒绝
        assert!(call(&state, "set_base_dir", json!({ "base_dir": "  " })).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    /// sync_repo 只验证拒绝路径（有效路径会拉起真实 git 同步，不在单测覆盖）：
    /// 仓库不存在 / 无源地址 / 无备份目标均拒绝且不启动同步
    #[test]
    fn mcp_sync_repo_rejects_unconfigured() {
        let (state, root) = open_state();
        assert!(
            call(&state, "sync_repo", json!({ "id": "nope" })).is_err(),
            "仓库不存在"
        );
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'r1', '')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r2', 'r2', 'https://src/r2.git')",
                [],
            )
            .unwrap();
        }
        assert!(call(&state, "sync_repo", json!({ "id": "r1" })).is_err(), "无源地址");
        assert!(
            call(&state, "sync_repo", json!({ "id": "r2" })).is_err(),
            "无备份目标"
        );
        // 三次拒绝均未入队：同步集合与队列保持为空
        assert!(lock(&state.syncing).is_empty());
        assert!(lock(&state.sync_queue).jobs.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// discover_repos：扫描基地址登记新仓库并返回列表，重复调用幂等
    #[test]
    fn discover_repos_scans_base_dir() {
        let (state, root) = open_state();
        // 基地址内放一个带 origin + backup 远端的 git 仓库（仅 .git/config，无需真实 git）
        let base = root.join("base");
        std::fs::create_dir_all(base.join("alpha/.git")).unwrap();
        std::fs::write(
            base.join("alpha/.git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/u/alpha.git\n\
             [remote \"backup\"]\n\turl = https://gitlab.com/u/alpha.git\n",
        )
        .unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());

        let repos = call(&state, "discover_repos", json!({})).unwrap();
        let arr = repos.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], json!("alpha"));
        assert_eq!(arr[0]["source"], json!("https://github.com/u/alpha.git"));
        let targets = arr[0]["targets"].as_array().unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["remote"], json!("backup"));

        // 幂等：再次扫描不产生重复
        let again = call(&state, "discover_repos", json!({})).unwrap();
        assert_eq!(again.as_array().unwrap().len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    /// sync_repos：选择器校验与批量结果报告（不触发真实同步的路径）
    #[test]
    fn mcp_sync_repos_requires_selector_and_reports_results() {
        let (state, root) = open_state();
        // 无 ids 且无 days：拒绝
        assert!(call(&state, "sync_repos", json!({})).is_err());
        // 空 ids 且无 days：拒绝
        assert!(call(&state, "sync_repos", json!({ "ids": [] })).is_err());
        // ids 列表：不存在与未配置的仓库均报告未启动
        let r = call(&state, "sync_repos", json!({ "ids": ["nope"] })).unwrap();
        assert_eq!(r["requested"], json!(1));
        assert_eq!(r["started"], json!(0));
        assert_eq!(r["results"][0]["id"], json!("nope"));
        assert_eq!(r["results"][0]["started"], json!(false));
        // days 范围：当前无任何已配置仓库，不启动任何同步
        let r = call(&state, "sync_repos", json!({ "days": 0 })).unwrap();
        assert_eq!(r["requested"], json!(0));
        assert_eq!(r["started"], json!(0));
        assert!(lock(&state.syncing).is_empty(), "未产生真实同步任务");
        std::fs::remove_dir_all(&root).ok();
    }
}
