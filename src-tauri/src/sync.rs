//! 同步任务：守护角色单执行者——持有 sync-daemon.lock 的进程是唯一的
//! 同步执行者，GUI 与 MCP 进程通过 SQLite 任务表（sync_jobs）投递任务，
//! 执行者单线程串行消费；停止请求经 sync_stop 表跨进程传递，由执行者的
//! git 轮询自查终止。git 子进程细节见 git.rs；仓库 CRUD 见 repos.rs。

use crate::auth::require_session;
use crate::git::perform_git_sync;
use crate::lang::{gui_lang, tr, tr_a, Lang};
use crate::state::{lock, now_ms, repo_from_row, AppState, REPO_COLS};
use fs2::FileExt;
use rusqlite::params;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::State;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub id: String,
    pub name: String,
    pub status: String,
    pub message: Option<String>,
    pub last_synced: Option<i64>,
}

/// 排队任务（执行者从 sync_jobs 表认领后执行）
struct Job {
    id: i64,
    repo_id: String,
    operator: String,
    lang: Lang,
}

fn emit_status(state: &AppState, ev: SyncEvent) {
    state.emit(ev);
}

#[tauri::command]
pub fn start_sync(
    state: State<'_, Arc<AppState>>,
    token: String,
    ids: Vec<String>,
) -> Result<usize, String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let results = enqueue_syncs(state.inner(), &ids, &username, lang);
    Ok(results.iter().filter(|(_, started)| *started).count())
}

#[tauri::command]
pub fn stop_sync(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: Option<String>,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let (dequeued, flagged) = stop_jobs(state.inner(), id.as_ref().map(std::slice::from_ref));
    if !dequeued.is_empty() || !flagged.is_empty() {
        state.add_log(&tr(lang, "log-sync-stopped-cmd"), &username);
    }
    Ok(())
}

/// 停止同步（GUI 命令与 MCP stop_syncs 工具共用）：排队任务直接出队；
/// 运行中任务写入停止标记，由执行者进程的 git 轮询自查后终止——停止
/// 请求跨进程生效。返回（出队的仓库 id，标记停止的仓库 id）。
/// ids 为 None 表示全部活动任务。
pub fn stop_jobs(state: &AppState, ids: Option<&[String]>) -> (Vec<String>, Vec<String>) {
    let conn = lock(&state.conn);
    let rows: Vec<(String, String)> = {
        let mut stmt = match conn
            .prepare("SELECT repo_id, state FROM sync_jobs WHERE state IN ('queued', 'running') ORDER BY id")
        {
            Ok(s) => s,
            Err(_) => return (Vec::new(), Vec::new()),
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    let mut dequeued = Vec::new();
    let mut flagged = Vec::new();
    for (repo_id, st) in rows {
        if let Some(ids) = ids {
            if !ids.iter().any(|i| i == &repo_id) {
                continue;
            }
        }
        if st == "queued" {
            // 排队任务尚未运行：直接出队
            let _ = conn.execute(
                "DELETE FROM sync_jobs WHERE repo_id = ?1 AND state = 'queued'",
                params![repo_id],
            );
            dequeued.push(repo_id);
        } else {
            // 运行中：写停止标记，执行者的 git 轮询自查后终止
            let _ = conn.execute(
                "INSERT OR IGNORE INTO sync_stop (repo_id) VALUES (?1)",
                params![repo_id],
            );
            flagged.push(repo_id);
        }
    }
    (dequeued, flagged)
}

/// 批量投递同步任务（界面与 MCP 共用）：未配置（缺源地址或备份目标）的
/// 仓库跳过；同仓库已有排队/运行中任务时被唯一部分索引拒绝（跨进程去重）。
/// 投递前尝试竞选守护角色（无执行者时由本进程接管）。
pub fn enqueue_syncs(
    state: &Arc<AppState>,
    ids: &[String],
    operator: &str,
    lang: Lang,
) -> Vec<(String, bool)> {
    state.try_become_daemon();
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        // 未配置（缺源地址或备份目标）的仓库不参与同步，避免必然的“失败”
        let configured = {
            let conn = lock(&state.conn);
            let src: String = conn
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
            !src.is_empty() && n > 0
        };
        if !configured {
            results.push((id.clone(), false));
            continue;
        }
        let inserted = {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                 VALUES (?1, ?2, ?3, 'queued', ?4)",
                params![id, operator, lang.as_str(), now_ms()],
            )
            .is_ok()
        };
        results.push((id.clone(), inserted));
    }
    results
}

impl AppState {
    /// 竞选同步执行者：持有 sync-daemon.lock 者获胜（锁已被其他进程持有时
    /// 返回 false，本进程保持客户端角色）。获胜即接管——复位上一任残留的
    /// running 任务与停止标记，并启动串行消费线程；角色保持到进程退出，
    /// 锁由 OS 释放，存活进程随后接管。
    pub fn try_become_daemon(self: &Arc<Self>) -> bool {
        {
            let mut guard = lock(&self.daemon);
            if guard.is_some() {
                return true;
            }
            let file = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&self.daemon_lock_path)
            {
                Ok(f) => f,
                Err(_) => return false,
            };
            if file.try_lock_exclusive().is_err() {
                return false;
            }
            *guard = Some(file);
        }
        // 接管：上一任执行者可能死于同步中途，残留状态就地复位
        {
            let conn = lock(&self.conn);
            let _ = conn.execute("DELETE FROM sync_jobs WHERE state = 'running'", []);
            let _ = conn.execute("DELETE FROM sync_stop", []);
            let _ = conn.execute(
                "UPDATE repos SET last_status = 'idle' WHERE last_status = 'running'",
                [],
            );
        }
        let st = self.clone();
        thread::spawn(move || daemon_worker(st));
        true
    }
}

/// 执行者主循环：单线程串行消费任务表（避免并发拉取抢占网络）
fn daemon_worker(state: Arc<AppState>) {
    loop {
        let Some(job) = claim_next_job(&state) else {
            thread::sleep(Duration::from_millis(200));
            continue;
        };
        run_sync(&state, &job.repo_id, &job.operator, job.lang);
        // 任务行与停止标记清理（停止标记残留会阻断该仓库的下次同步）
        let conn = lock(&state.conn);
        let _ = conn.execute("DELETE FROM sync_jobs WHERE id = ?1", params![job.id]);
        let _ = conn.execute(
            "DELETE FROM sync_stop WHERE repo_id = ?1",
            params![job.repo_id],
        );
    }
}

/// 原子认领最早的排队任务（事务内置为 running，防重复消费）
fn claim_next_job(state: &AppState) -> Option<Job> {
    let mut conn = lock(&state.conn);
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .ok()?;
    let job = tx
        .query_row(
            "SELECT id, repo_id, operator, lang FROM sync_jobs
             WHERE state = 'queued' ORDER BY id LIMIT 1",
            [],
            |r| {
                Ok(Job {
                    id: r.get(0)?,
                    repo_id: r.get(1)?,
                    operator: r.get(2)?,
                    lang: Lang::parse_tag(Some(&r.get::<_, String>(3)?)).unwrap_or_default(),
                })
            },
        )
        .ok()?;
    tx.execute(
        "UPDATE sync_jobs SET state = 'running' WHERE id = ?1",
        params![job.id],
    )
    .ok()?;
    tx.commit().ok()?;
    Some(job)
}

/// 客户端后台线程（本进程未持有执行者角色时）：轮询仓库状态、对变化合成
/// sync-status 事件（执行者在其他进程时界面实时性不变），并尝试竞选执行
/// 者——原执行者退出后接管，成功即退出。执行者进程不启动本线程。
pub fn spawn_client_loop(state: Arc<AppState>) {
    thread::spawn(move || {
        let mut last: HashMap<String, (String, Option<String>, Option<i64>)> = HashMap::new();
        let mut primed = false;
        while !state.is_daemon() {
            let rows: Vec<SyncEvent> = {
                let conn = lock(&state.conn);
                let queried: Vec<SyncEvent> = match conn.prepare(
                    "SELECT id, name, last_status, last_message, last_synced
                     FROM repos WHERE hidden = 0",
                ) {
                    Ok(mut stmt) => stmt
                        .query_map([], |r| {
                            Ok(SyncEvent {
                                id: r.get(0)?,
                                name: r.get(1)?,
                                status: r.get(2)?,
                                message: r.get(3)?,
                                last_synced: r.get(4)?,
                            })
                        })
                        .map(|rows| rows.filter_map(Result::ok).collect())
                        .unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                queried
            };
            for ev in &rows {
                let prev = last.get(&ev.id);
                if primed
                    && prev != Some(&(ev.status.clone(), ev.message.clone(), ev.last_synced))
                {
                    state.emit(ev.clone());
                }
            }
            last = rows
                .into_iter()
                .map(|ev| (ev.id, (ev.status, ev.message, ev.last_synced)))
                .collect();
            primed = true;
            thread::sleep(Duration::from_millis(800));
        }
    });
}

/// 按范围选择参与同步的仓库（与界面范围下拉语义一致）：
/// days <= 0 为全部已配置仓库；days > 0 为最近 N 天未同步（含从未同步）
pub fn select_stale_ids(state: &AppState, days: i64) -> Result<Vec<String>, String> {
    let conn = lock(&state.conn);
    let mut sql = String::from(
        "SELECT id FROM repos WHERE hidden = 0 AND source != ''
         AND EXISTS (SELECT 1 FROM sync_targets WHERE sync_targets.repo_id = repos.id)",
    );
    let cutoff = now_ms() - days.saturating_mul(86_400_000);
    let args: Vec<i64> = if days > 0 {
        sql.push_str(" AND (last_synced IS NULL OR last_synced < ?1)");
        vec![cutoff]
    } else {
        Vec::new()
    };
    sql.push_str(" ORDER BY name");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

fn set_running(state: &AppState, repo_id: &str) {
    let conn = lock(&state.conn);
    let _ = conn.execute(
        "UPDATE repos SET last_status = 'running', last_message = NULL WHERE id = ?1",
        params![repo_id],
    );
    let _ = conn.execute(
        "UPDATE sync_targets SET last_status = 'running', last_message = NULL WHERE repo_id = ?1",
        params![repo_id],
    );
}

fn finish(
    state: &AppState,
    repo_name: &str,
    repo_id: &str,
    status: &str,
    message: Option<String>,
    update_time: bool,
    operator: &str,
    lang: Lang,
) {
    {
        let conn = lock(&state.conn);
        if update_time {
            let _ = conn.execute(
                "UPDATE repos SET last_status = ?1, last_message = ?2, last_synced = ?3 WHERE id = ?4",
                params![status, message, now_ms(), repo_id],
            );
        } else {
            let _ = conn.execute(
                "UPDATE repos SET last_status = ?1, last_message = ?2 WHERE id = ?3",
                params![status, message, repo_id],
            );
        }
    }
    let action = match status {
        "success" => tr_a(lang, "log-sync-success", &[("name", repo_name)]),
        "stopped" => tr_a(lang, "log-sync-stopped", &[("name", repo_name)]),
        // 失败日志附带原因（git 错误或按目标推送错误），超长截断保持日志可读
        _ => match message.as_deref() {
            Some(reason) if !reason.is_empty() => tr_a(
                lang,
                "log-sync-failed-reason",
                &[("name", repo_name), ("reason", &truncate_reason(reason))],
            ),
            _ => tr_a(lang, "log-sync-failed", &[("name", repo_name)]),
        },
    };
    state.add_log(&action, operator);
}

/// 失败原因写入日志前折叠空白并按字符数截断（git 错误输出可能很长），保持日志单行可读
fn truncate_reason(msg: &str) -> String {
    const MAX_CHARS: usize = 300;
    let flat = msg.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_CHARS {
        return flat;
    }
    let mut s: String = flat.chars().take(MAX_CHARS).collect();
    s.push('…');
    s
}

fn run_sync(state: &AppState, repo_id: &str, operator: &str, lang: Lang) {
    let repo = {
        let conn = lock(&state.conn);
        conn.query_row(
            &format!("SELECT {REPO_COLS} FROM repos WHERE id = ?1"),
            params![repo_id],
            repo_from_row,
        )
        .ok()
    };
    let Some(repo) = repo else { return };

    set_running(state, repo_id);
    emit_status(
        state,
        SyncEvent {
            id: repo_id.to_string(),
            name: repo.name.clone(),
            status: "running".into(),
            message: None,
            last_synced: repo.last_synced,
        },
    );

    let targets: Vec<(String, String)> = {
        let conn = lock(&state.conn);
        // 查询失败按无目标处理，由下方“未配置”检查给出可读错误
        let rows = match conn
            .prepare("SELECT remote, url FROM sync_targets WHERE repo_id = ?1 ORDER BY remote")
        {
            Ok(mut stmt) => stmt
                .query_map(params![repo_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        rows
    };

    if repo.source.is_empty() {
        let msg = tr(lang, "sync-no-source");
        finish(
            state,
            &repo.name,
            repo_id,
            "failed",
            Some(msg.clone()),
            false,
            operator,
            lang,
        );
        emit_status(
            state,
            SyncEvent {
                id: repo_id.to_string(),
                name: repo.name.clone(),
                status: "failed".into(),
                message: Some(msg),
                last_synced: repo.last_synced,
            },
        );
        return;
    }
    if targets.is_empty() {
        let msg = tr(lang, "sync-no-targets");
        finish(
            state,
            &repo.name,
            repo_id,
            "failed",
            Some(msg.clone()),
            false,
            operator,
            lang,
        );
        emit_status(
            state,
            SyncEvent {
                id: repo_id.to_string(),
                name: repo.name.clone(),
                status: "failed".into(),
                message: Some(msg),
                last_synced: repo.last_synced,
            },
        );
        return;
    }

    state.add_log(
        &tr_a(lang, "log-sync-started", &[("name", &repo.name)]),
        operator,
    );
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let (status, message) =
        perform_git_sync(state, repo_id, &repo, &targets, &Path::new(&base_dir), lang);
    let success = status == "success";
    finish(
        state,
        &repo.name,
        repo_id,
        &status,
        Some(message.clone()),
        success,
        operator,
        lang,
    );
    // 未执行到推送的目标行（如克隆/拉取阶段失败）从 running 复位
    crate::git::reset_running_targets(state, repo_id);
    emit_status(
        state,
        SyncEvent {
            id: repo_id.to_string(),
            name: repo.name.clone(),
            status,
            message: Some(message),
            last_synced: if success { Some(now_ms()) } else { repo.last_synced },
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::time::Instant;

    /// select_stale_ids 与界面范围语义一致：0=全部已配置，N=最近 N 天未同步（含从未同步）；
    /// 未配置（缺源/缺目标）与隐藏仓库不参与
    #[test]
    fn select_stale_ids_matches_range_semantics() {
        let root = std::env::temp_dir().join(format!("grs-stale-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::open(root.join("app.db")).unwrap();
        {
            let conn = lock(&state.conn);
            let now = now_ms();
            let day: i64 = 86_400_000;
            // (id, source, last_synced, hidden)；有目标的仓库统一补一条 backup 目标
            let rows = [
                ("fresh", "https://src/f.git", Some(now), 0),
                ("stale", "https://src/s.git", Some(now - 10 * day), 0),
                ("never", "https://src/n.git", None, 0),
                ("unconf", "", None, 0),
                ("notarget", "https://src/t.git", None, 0),
                ("hidden", "https://src/h.git", None, 1),
            ];
            for (id, source, synced, hidden) in rows {
                conn.execute(
                    "INSERT INTO repos (id, name, source, last_synced, hidden) VALUES (?1, ?1, ?2, ?3, ?4)",
                    params![id, source, synced, hidden],
                )
                .unwrap();
                if id != "notarget" {
                    conn.execute(
                        "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, 'backup', 'https://bak/x.git')",
                        params![id],
                    )
                    .unwrap();
                }
            }
        }
        assert_eq!(
            select_stale_ids(&state, 0).unwrap(),
            vec!["fresh", "never", "stale"],
            "全部范围 = 已配置仓库（按名称排序）"
        );
        assert_eq!(
            select_stale_ids(&state, 7).unwrap(),
            vec!["never", "stale"],
            "7 天范围 = 从未同步 + 超期仓库"
        );
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// enqueue_syncs：未配置仓库跳过且不投递任务
    #[test]
    fn enqueue_syncs_skips_unconfigured() {
        let root = std::env::temp_dir().join(format!("grs-enq-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        {
            let conn = lock(&state.conn);
            conn.execute("INSERT INTO repos (id, name, source) VALUES ('r1', 'r1', '')", [])
                .unwrap();
        }
        let ids = ["r1".to_string(), "nope".to_string()];
        let results = enqueue_syncs(&state, &ids, "test", crate::lang::Lang::Zh);
        assert_eq!(
            results,
            vec![("r1".to_string(), false), ("nope".to_string(), false)]
        );
        let n: i64 = {
            let conn = lock(&state.conn);
            conn.query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(n, 0, "未产生任何同步任务");
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 同仓库同时至多一个排队/运行中任务：唯一部分索引跨进程原子去重
    #[test]
    fn duplicate_active_jobs_rejected_by_unique_index() {
        let root = std::env::temp_dir().join(format!("grs-dupjob-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::open(root.join("app.db")).unwrap();
        let conn = lock(&state.conn);
        conn.execute(
            "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
             VALUES ('r1', 'other', 'zh', 'queued', 0)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                 VALUES ('r1', 'test', 'zh', 'queued', 1)",
                [],
            )
            .is_err(),
            "同仓库的第二个活动任务应被唯一索引拒绝"
        );
        // 排队任务出队后允许再次投递
        conn.execute("DELETE FROM sync_jobs", []).unwrap();
        assert!(conn
            .execute(
                "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                 VALUES ('r1', 'test', 'zh', 'queued', 2)",
                [],
            )
            .is_ok());
        drop(conn);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 失败日志附带原因：缺源地址的仓库走完整 run_sync 后，
    /// 操作日志应记录包含可读原因（sync-no-source 文案）的失败条目
    #[test]
    fn failed_sync_logs_reason() {
        use crate::lang::tr;

        let root = std::env::temp_dir().join(format!("grs-faillog-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::open(root.join("app.db")).unwrap();
        {
            let conn = lock(&state.conn);
            conn.execute("INSERT INTO repos (id, name, source) VALUES ('r1', 'demo', '')", [])
                .unwrap();
        }
        run_sync(&state, "r1", "test", crate::lang::Lang::Zh);

        let reason = tr(crate::lang::Lang::Zh, "sync-no-source");
        let actions: Vec<String> = {
            let conn = lock(&state.conn);
            let mut stmt = conn.prepare("SELECT action FROM operation_logs").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        assert!(
            actions
                .iter()
                .any(|a| a.contains("demo") && a.contains(&reason)),
            "失败日志应包含原因（{reason}）: {actions:?}"
        );

        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 截断函数：空白折叠为单空格，超长按字符截断并带省略号
    #[test]
    fn truncate_reason_collapses_and_caps() {
        assert_eq!(truncate_reason("a\n\tb   c"), "a b c");
        let long = "x".repeat(1000);
        let t = truncate_reason(&long);
        assert_eq!(t.chars().count(), 301);
        assert!(t.ends_with('…'));
        // 多字节字符按字符截断，不产生半截 UTF-8
        let wide = "仓".repeat(500);
        assert_eq!(truncate_reason(&wide).chars().count(), 301);
    }

    /// 端到端（执行者单例）：竞选守护角色后投递任务，单线程消费执行
    /// 真实 git 流水线，目标 bare 仓库推进到源 HEAD、状态成功、任务行清空
    #[test]
    fn daemon_executes_enqueued_sync_end_to_end() {
        let root = std::env::temp_dir().join(format!("grs-daemon-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("src");
        let target = root.join("target.git");
        let git = |args: &[&str], cwd: &Path| {
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

        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
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
        assert!(state.try_become_daemon());
        let results = enqueue_syncs(&state, &["r1".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(results, vec![("r1".to_string(), true)]);

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
            thread::sleep(Duration::from_millis(100));
        }
        // 目标 bare 仓库已推进到源 HEAD（裸仓库 HEAD 可能指向 unborn 的
        // 默认分支，显式解析 refs/heads/main）
        let rev = |cwd: &Path, spec: &str| {
            let out = std::process::Command::new("git")
                .args(["rev-parse", spec])
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(rev(&target, "refs/heads/main"), rev(&source, "HEAD"), "目标与源提交一致");
        // 停止标记未残留
        assert!(!state.stop_requested("r1"));
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 停止同步（命令层）：运行中任务写停止标记后被执行者终止为 stopped，
    /// 排队任务直接出队；停止标记由收尾清理
    #[test]
    fn stop_sync_stops_running_and_dequeues_queued() {
        use crate::state::testutil::{insert_session, open_mock_app};
        use tauri::Manager;

        let (app, state, root) = open_mock_app("stop");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");

        // 仓库源指向一个 git 子命令：pre-auto-gc hook 读 stdin 挂起，
        // 保证子进程存活可被停止
        let repo_dir = root.join("busy");
        std::fs::create_dir_all(&repo_dir).unwrap();
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'busy', ?1)",
                params![repo_dir.to_string_lossy()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_targets (repo_id, remote, url) VALUES ('r1', 'backup', ?1)",
                params![repo_dir.join("b.git").to_string_lossy()],
            )
            .unwrap();
            // r2 仅入库为已配置（无本地目录）：worker 忙于 r1 时排队
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r2', 'r2', 'https://src/r2.git')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_targets (repo_id, remote, url) VALUES ('r2', 'backup', 'https://bak/r2.git')",
                [],
            )
            .unwrap();
        }
        let git = |args: &[&str]| {
            let s = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(&repo_dir)
                .status()
                .unwrap();
            assert!(s.success(), "git {:?} 失败", args);
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "user.email", "t@t"]);
        std::fs::write(repo_dir.join("a.txt"), "v").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "v"]);
        let hooks = repo_dir.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        #[cfg(windows)]
        let hook_body = "@echo off\r\nmore\r\n";
        #[cfg(not(windows))]
        let hook_body = "#!/bin/sh\ncat\n";
        std::fs::write(hooks.join("pre-auto-gc"), hook_body).unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(hooks.join("pre-auto-gc"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }

        // 竞选执行者并投递：r1 运行（挂起）、r2 排队
        assert!(state.try_become_daemon());
        let results = enqueue_syncs(&state, &["r1".to_string(), "r2".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(
            results,
            vec![("r1".to_string(), true), ("r2".to_string(), true)]
        );
        // 重复投递被唯一索引拒绝
        let dup = enqueue_syncs(&state, &["r1".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(dup, vec![("r1".to_string(), false)], "运行中的仓库不应重复投递");

        // 等 r1 进入运行态后再等一个轮询周期，确保 git 已挂起
        let mut waited = 0;
        loop {
            let status: String = {
                let conn = lock(&state.conn);
                conn.query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| r.get(0))
                    .unwrap_or_default()
            };
            if status == "running" {
                break;
            }
            if waited > 200 {
                panic!("同步未进入运行态");
            }
            thread::sleep(Duration::from_millis(50));
            waited += 1;
        }
        thread::sleep(Duration::from_millis(600));

        // 停止全部：r1 标记停止（跨进程语义）、r2 排队出队
        stop_sync(st.clone(), "tok".into(), None).unwrap();
        let mut status = String::new();
        for _ in 0..200 {
            status = {
                let conn = lock(&state.conn);
                conn.query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap_or_default()
            };
            let jobs: i64 = {
                let conn = lock(&state.conn);
                conn.query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                    .unwrap_or(-1)
            };
            if status == "stopped" && jobs == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(status, "stopped", "停止后仓库状态应为 stopped");
        // 停止标记已由收尾清理，不阻断下次同步
        assert!(!state.stop_requested("r1"), "停止标记应已清理");
        // 操作日志含停止记录
        let logs: i64 = {
            let conn = lock(&state.conn);
            conn.query_row(
                "SELECT COUNT(*) FROM operation_logs WHERE action LIKE '%停止%'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
        };
        assert!(logs >= 1, "停止应写操作日志");
        drop(st);
        drop(app);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 跨进程停止：另一进程直接写 sync_stop 表（stop_syncs 的实际效果），
    /// 执行者的 git 轮询自查后终止为 stopped
    #[test]
    fn stop_flag_from_another_process_still_stops() {
        use crate::state::AppState;

        let root = std::env::temp_dir().join(format!("grs-stopflag-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let source_repo = root.join("busy");
        std::fs::create_dir_all(&source_repo).unwrap();
        let git = |args: &[&str]| {
            let s = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(&source_repo)
                .status()
                .unwrap();
            assert!(s.success(), "git {:?} 失败", args);
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "user.email", "t@t"]);
        std::fs::write(source_repo.join("a.txt"), "v").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "v"]);
        let hooks = source_repo.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        #[cfg(windows)]
        let hook_body = "@echo off\r\nmore\r\n";
        #[cfg(not(windows))]
        let hook_body = "#!/bin/sh\ncat\n";
        std::fs::write(hooks.join("pre-auto-gc"), hook_body).unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(hooks.join("pre-auto-gc"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }

        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'busy', ?1)",
                params![source_repo.to_string_lossy()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_targets (repo_id, remote, url) VALUES ('r1', 'backup', ?1)",
                params![source_repo.join("b.git").to_string_lossy()],
            )
            .unwrap();
        }
        assert!(state.try_become_daemon());
        let results = enqueue_syncs(&state, &["r1".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(results, vec![("r1".to_string(), true)]);
        let mut waited = 0;
        loop {
            let status: String = {
                let conn = lock(&state.conn);
                conn.query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| r.get(0))
                    .unwrap_or_default()
            };
            if status == "running" {
                break;
            }
            if waited > 200 {
                panic!("同步未进入运行态");
            }
            thread::sleep(Duration::from_millis(50));
            waited += 1;
        }
        thread::sleep(Duration::from_millis(600));

        // 另一进程的停止方式：直接写 sync_stop 表（无本进程内存态参与）
        {
            let conn = lock(&state.conn);
            conn.execute("INSERT OR IGNORE INTO sync_stop (repo_id) VALUES ('r1')", [])
                .unwrap();
        }
        let mut status = String::new();
        for _ in 0..200 {
            status = {
                let conn = lock(&state.conn);
                conn.query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap_or_default()
            };
            if status == "stopped" && !state.stop_requested("r1") {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(status, "stopped", "他方写入的停止标记应终止同步");
        assert!(!state.stop_requested("r1"), "停止标记应已清理");
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 守护接管：上一任执行者死于同步中途残留的 running 任务、停止标记
    /// 与 running 状态，由接管者在竞选成功时复位
    #[test]
    fn daemon_takeover_resets_stale_state() {
        let root = std::env::temp_dir().join(format!("grs-takeover-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("app.db");
        {
            let a = Arc::new(AppState::open(db.clone()).unwrap());
            {
                let conn = lock(&a.conn);
                conn.execute(
                    "INSERT INTO repos (id, name, source, last_status) VALUES ('r1', 'r1', 'https://src/r1.git', 'running')",
                    [],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                     VALUES ('r1', 'test', 'zh', 'running', 0)",
                    [],
                )
                .unwrap();
                conn.execute("INSERT INTO sync_stop (repo_id) VALUES ('r1')", []).unwrap();
            }
            // a 未竞选守护即释放（模拟上一任执行者崩溃后的残留）
        }
        let b = Arc::new(AppState::open(db.clone()).unwrap());
        assert!(b.try_become_daemon(), "锁已释放，接管应成功");
        let (jobs, flags, status): (i64, i64, String) = {
            let conn = lock(&b.conn);
            let jobs: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sync_jobs WHERE state = 'running'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let flags: i64 = conn
                .query_row("SELECT COUNT(*) FROM sync_stop", [], |r| r.get(0))
                .unwrap();
            let status: String = conn
                .query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| r.get(0))
                .unwrap();
            (jobs, flags, status)
        };
        assert_eq!(jobs, 0, "残留 running 任务应被复位");
        assert_eq!(flags, 0, "残留停止标记应被清理");
        assert_eq!(status, "idle", "running 仓库状态应复位");
        drop(b);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 客户端轮询线程：执行者在其他进程时，本进程对仓库状态变化合成
    /// sync-status 事件（首帧快照静默，其后变化才发）
    #[test]
    fn client_loop_synthesizes_status_events() {
        let root = std::env::temp_dir().join(format!("grs-poll-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'demo', 'https://src/r1.git')",
                [],
            )
            .unwrap();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        state.set_emitter(Box::new(move |ev| {
            let _ = tx.send(ev);
        }));
        assert!(!state.is_daemon());
        spawn_client_loop(state.clone());

        // 首帧快照后变更状态（模拟执行者进程写入）
        thread::sleep(Duration::from_millis(100));
        {
            let conn = lock(&state.conn);
            conn.execute(
                "UPDATE repos SET last_status = 'running' WHERE id = 'r1'",
                [],
            )
            .unwrap();
        }
        let ev = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("应收到合成的状态事件");
        assert_eq!(ev.id, "r1");
        assert_eq!(ev.name, "demo");
        assert_eq!(ev.status, "running");
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }
}
