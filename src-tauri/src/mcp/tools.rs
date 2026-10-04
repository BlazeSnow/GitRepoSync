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
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "update_repo",
                "description": tr(lang, "tool-update-repo"),
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "remove_repo",
                "description": tr(lang, "tool-remove-repo"),
                "inputSchema": { "type": "object", "properties": {} }
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
                "name": "stop_syncs",
                "description": tr(lang, "tool-stop-syncs"),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": tr(lang, "tool-stop-syncs-ids")
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
        // 仓库配置只读：三个历史管理工具保留名称作为兼容入口，调用返回
        // 对应的 git 操作指引（用户自行 clone / git remote 管理远端）
        "add_repo" | "update_repo" | "remove_repo" => {
            let base_dir = state
                .get_setting("base_dir")
                .unwrap_or_else(crate::state::default_base_dir);
            let key = match name {
                "update_repo" => "mcp-help-update-repo",
                "remove_repo" => "mcp-help-remove-repo",
                _ => "mcp-help-add-repo",
            };
            Ok(json!(tr_a(lang, key, &[("base_dir", &base_dir)])))
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
            let results = sync::enqueue_syncs(state, std::slice::from_ref(&id), OPERATOR, lang);
            let started = results.first().map(|(_, s)| *s).unwrap_or(false);
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
            // 未配置 / 已在同步的仓库报告未启动；真实同步由执行者串行消费
            let results = sync::enqueue_syncs(state, &ids, OPERATOR, lang);
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
        "stop_syncs" => {
            let ids_opt: Option<Vec<String>> = args
                .get("ids")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                });
            let (dequeued, flagged) = sync::stop_jobs(state, ids_opt.as_deref());
            if !dequeued.is_empty() || !flagged.is_empty() {
                state.add_log(&tr(lang, "log-sync-stopped-cmd"), OPERATOR);
            }
            Ok(json!({
                "dequeued": dequeued,
                "stopRequested": flagged,
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
    use uuid::Uuid;

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

    /// list_logs：按时间倒序返回操作日志，limit 生效（直接插入日志行：
    /// 仓库变更类 MCP 工具已只读化，不再产生日志）
    #[test]
    fn list_logs_returns_recent_entries() {
        let (state, root) = open_state();
        {
            let conn = lock(&state.conn);
            for (i, name) in ["l1", "l2"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO operation_logs (action, operator, created_at) VALUES (?1, 'mcp', ?2)",
                    params![format!("添加仓库「{name}」"), i as i64],
                )
                .unwrap();
            }
        }
        let logs = call(&state, "list_logs", json!({ "limit": 1 })).unwrap();
        let arr = logs.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["operator"], json!("mcp"));
        assert_eq!(arr[0]["action"], json!("添加仓库「l2」"), "倒序：最新操作在前");
        let all = call(&state, "list_logs", json!({})).unwrap();
        assert!(all.as_array().unwrap().len() >= 2);
        // limit 钳制到 [1, 1000]，非法值不报错
        let clamped = call(&state, "list_logs", json!({ "limit": 0 })).unwrap();
        assert!(!clamped.as_array().unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// base_dir 往返（~ 展开为完整路径）与未知工具
    #[test]
    fn mcp_base_dir_roundtrip() {
        let (state, root) = open_state();
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

    /// 仓库配置只读：add_repo / update_repo / remove_repo 保留名称作为
    /// 兼容入口，调用返回对应的 git 操作指引且不改任何数据
    #[test]
    fn mcp_repo_tools_return_help() {
        let (state, root) = open_state();
        for tool in ["add_repo", "update_repo", "remove_repo"] {
            let envelope = tools_call(
                &state,
                Lang::Zh,
                &json!({ "name": tool, "arguments": { "id": "x", "name": "x" } }),
            )
            .unwrap();
            assert_eq!(envelope["isError"], json!(false), "{tool} 应成功返回指引");
            let text = envelope["content"][0]["text"].as_str().unwrap();
            assert!(text.contains("sync_repo"), "{tool} 指引应引导使用同步工具: {text}");
            if tool != "remove_repo" {
                // 退登记指引不涉及远端操作
                assert!(text.contains("git remote"), "{tool} 指引应包含 git 操作说明: {text}");
            }
        }
        // 未产生任何数据变更
        let repos = call(&state, "list_repos", json!({})).unwrap();
        assert_eq!(repos.as_array().unwrap().len(), 0);
        let jobs: i64 = {
            let conn = lock(&state.conn);
            conn.query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(jobs, 0);
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
        // 三次拒绝均未入队：任务表保持为空
        let jobs: i64 = {
            let conn = lock(&state.conn);
            conn.query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(jobs, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    /// sync_repo 端到端：经由工具层投递任务、执行者消费执行真实流水线，
    /// 目标 bare 仓库推进到源 HEAD，操作人以 mcp 记录
    #[test]
    fn mcp_sync_repo_executes_end_to_end() {
        use std::time::{Duration, Instant};

        let (state, root) = open_state();
        let base = root.join("base");
        let source = root.join("src");
        let target = root.join("target.git");
        let git = |args: &[&str], cwd: &std::path::Path| {
            let s = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(s.success(), "git {:?} 执行失败", args);
        };
        std::fs::create_dir_all(&source).unwrap();
        git(&["init", "-q", "-b", "main"], &source);
        git(&["config", "user.name", "t"], &source);
        git(&["config", "user.email", "t@t"], &source);
        std::fs::write(source.join("a.txt"), "v1").unwrap();
        git(&["add", "."], &source);
        git(&["commit", "-q", "-m", "v1"], &source);
        git(&["init", "-q", "--bare", target.to_str().unwrap()], &root);

        state.set_setting("base_dir", &base.to_string_lossy());
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'demo', ?1)",
                params![source.to_string_lossy()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_targets (repo_id, remote, url) VALUES ('r1', 'backup', ?1)",
                params![target.to_string_lossy()],
            )
            .unwrap();
        }

        let r = call(&state, "sync_repo", json!({ "id": "r1" })).unwrap();
        assert_eq!(r["started"], json!(true), "已配置仓库应成功投递");

        // 轮询：任务被消费、状态成功
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (jobs, status): (i64, String) = {
                let conn = lock(&state.conn);
                let jobs: i64 = conn
                    .query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                    .unwrap_or(-1);
                let status: String = conn
                    .query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| r.get(0))
                    .unwrap_or_default();
                (jobs, status)
            };
            if jobs == 0 && status == "success" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "任务未被消费或未成功: jobs={jobs} status={status}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        // 目标推进到源 HEAD
        let rev = |cwd: &std::path::Path, spec: &str| {
            let out = std::process::Command::new("git")
                .args(["rev-parse", spec])
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(rev(&target, "refs/heads/main"), rev(&source, "HEAD"));
        // 操作人以 mcp 记录（触发与同步结果）
        let rows: i64 = {
            let conn = lock(&state.conn);
            conn.query_row(
                "SELECT COUNT(*) FROM operation_logs WHERE operator = 'mcp'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(rows >= 2, "触发与同步结果应以 mcp 为操作人入库: rows={rows}");
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
        let jobs: i64 = {
            let conn = lock(&state.conn);
            conn.query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(jobs, 0, "未产生真实同步任务");
        std::fs::remove_dir_all(&root).ok();
    }

    /// stop_syncs：排队任务直接出队、运行中任务写停止标记（跨进程语义），
    /// 未指定 ids 时作用于全部活动任务
    #[test]
    fn stop_syncs_tool_dequeues_and_flags() {
        let (state, root) = open_state();
        // 直接插入任务行（不竞选执行者，避免 worker 拉起真实同步）
        {
            let conn = lock(&state.conn);
            for (id, st) in [("r1", "running"), ("r2", "queued")] {
                conn.execute(
                    "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                     VALUES (?1, 'mcp', 'zh', ?2, 0)",
                    rusqlite::params![id, st],
                )
                .unwrap();
            }
        }
        // 指定 ids：仅作用于 r2（排队 → 出队）
        let r = call(&state, "stop_syncs", json!({ "ids": ["r2"] })).unwrap();
        assert_eq!(r["dequeued"], json!(["r2"]));
        assert_eq!(r["stopRequested"], json!([]));
        let active = |id: &str| {
            let conn = lock(&state.conn);
            conn.query_row(
                "SELECT 1 FROM sync_jobs WHERE repo_id = ?1 AND state IN ('queued', 'running')",
                params![id],
                |_| Ok(()),
            )
            .is_ok()
        };
        assert!(!active("r2"));
        assert!(active("r1"), "运行中任务不受他人 ids 请求影响");

        // 全部停止：运行中的 r1 被标记
        let r = call(&state, "stop_syncs", json!({})).unwrap();
        assert_eq!(r["stopRequested"], json!(["r1"]));
        assert!(state.stop_requested("r1"));

        // 日志以 mcp 为操作人写入
        let operator: String = {
            let conn = lock(&state.conn);
            conn.query_row(
                "SELECT operator FROM operation_logs ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(operator, "mcp");
        std::fs::remove_dir_all(&root).ok();
    }
}
