//! 同步任务：守护角色单执行者——持有 sync-daemon.lock 的进程是唯一的
//! 同步执行者，GUI 与 MCP 进程通过 SQLite 任务表（sync_jobs）投递任务，
//! 执行者单线程串行消费；停止请求经 sync_stop 表跨进程传递，由执行者的
//! git 轮询自查终止。git 子进程细节见 git.rs；仓库列表见 scan.rs。

use crate::auth::require_session;
use crate::git::perform_git_sync;
use crate::lang::{gui_lang, tr, tr_a, Lang};
use crate::scan;
use crate::state::{lock, now_ms, AppState};
use fs2::FileExt;
use rusqlite::params;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::State;

/// 同步状态事件：仓库身份为基地址下的目录名
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
    pub last_synced: Option<i64>,
}

/// 排队任务（执行者从 sync_jobs 表认领后执行）
struct Job {
    id: i64,
    repo_name: String,
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
    names: Vec<String>,
) -> Result<usize, String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let results = enqueue_syncs(state.inner(), &names, &username, lang);
    Ok(results.iter().filter(|(_, started)| *started).count())
}

#[tauri::command]
pub fn stop_sync(
    state: State<'_, Arc<AppState>>,
    token: String,
    name: Option<String>,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let (dequeued, flagged) =
        stop_jobs(state.inner(), name.as_ref().map(std::slice::from_ref));
    if !dequeued.is_empty() || !flagged.is_empty() {
        state.add_log(&tr(lang, "log-sync-stopped-cmd"), &username);
    }
    Ok(())
}

/// 停止同步（GUI 命令与 MCP stop_syncs 工具共用）：排队任务直接出队；
/// 运行中任务写入停止标记，由执行者进程的 git 轮询自查后终止——停止
/// 请求跨进程生效。返回（出队的仓库名，标记停止的仓库名）。
/// names 为 None 表示全部活动任务。
pub fn stop_jobs(state: &AppState, names: Option<&[String]>) -> (Vec<String>, Vec<String>) {
    let conn = lock(&state.conn);
    let rows: Vec<(String, String)> = {
        let mut stmt = match conn.prepare(
            "SELECT repo_name, state FROM sync_jobs WHERE state IN ('queued', 'running') ORDER BY id",
        ) {
            Ok(s) => s,
            Err(_) => return (Vec::new(), Vec::new()),
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    let mut dequeued = Vec::new();
    let mut flagged = Vec::new();
    for (repo_name, st) in rows {
        if let Some(names) = names {
            if !names.iter().any(|n| n == &repo_name) {
                continue;
            }
        }
        if st == "queued" {
            // 排队任务尚未运行：直接出队
            let _ = conn.execute(
                "DELETE FROM sync_jobs WHERE repo_name = ?1 AND state = 'queued'",
                params![repo_name],
            );
            dequeued.push(repo_name);
        } else {
            // 运行中：写停止标记，执行者的 git 轮询自查后终止
            let _ = conn.execute(
                "INSERT OR IGNORE INTO sync_stop (repo_name) VALUES (?1)",
                params![repo_name],
            );
            flagged.push(repo_name);
        }
    }
    (dequeued, flagged)
}

/// 批量投递同步任务（界面与 MCP 共用）：未配置（缺源地址或备份目标）的
/// 仓库跳过；同仓库已有排队/运行中任务时被唯一部分索引拒绝（跨进程去重）。
/// 投递前尝试竞选守护角色（无执行者时由本进程接管）。
/// names 为仓库目录名（仓库身份），配置实时扫描 `.git/config` 判定。
pub fn enqueue_syncs(
    state: &Arc<AppState>,
    names: &[String],
    operator: &str,
    lang: Lang,
) -> Vec<(String, bool)> {
    state.try_become_daemon();
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let base = Path::new(&base_dir);
    let mut results = Vec::with_capacity(names.len());
    for name in names {
        // 未配置（缺源地址或备份目标）的仓库不参与同步，避免必然的“失败”；
        // 物理目录已不存在的仓库同样跳过（扫描不到）
        let configured = match scan::scan_one(base, name) {
            Some(r) => !r.source.is_empty() && !r.targets.is_empty(),
            None => false,
        };
        if !configured {
            results.push((name.clone(), false));
            continue;
        }
        let inserted = {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO sync_jobs (repo_name, operator, lang, state, created_at)
                 VALUES (?1, ?2, ?3, 'queued', ?4)",
                params![name, operator, lang.as_str(), now_ms()],
            )
            .is_ok()
        };
        results.push((name.clone(), inserted));
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
                "UPDATE repo_state SET last_status = 'idle' WHERE last_status = 'running'",
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
        run_sync(&state, &job.repo_name, &job.operator, job.lang);
        // 任务行与停止标记清理（停止标记残留会阻断该仓库的下次同步）
        let conn = lock(&state.conn);
        let _ = conn.execute("DELETE FROM sync_jobs WHERE id = ?1", params![job.id]);
        let _ = conn.execute(
            "DELETE FROM sync_stop WHERE repo_name = ?1",
            params![job.repo_name],
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
            "SELECT id, repo_name, operator, lang FROM sync_jobs
             WHERE state = 'queued' ORDER BY id LIMIT 1",
            [],
            |r| {
                Ok(Job {
                    id: r.get(0)?,
                    repo_name: r.get(1)?,
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
                let queried: Vec<SyncEvent> = {
                    let states = crate::state::load_repo_states(&conn);
                    states
                        .into_iter()
                        .map(|(name, st)| SyncEvent {
                            name,
                            status: st.last_status,
                            message: st.last_message,
                            last_synced: st.last_synced,
                        })
                        .collect()
                };
                queried
            };
            for ev in &rows {
                let prev = last.get(&ev.name);
                if primed
                    && prev != Some(&(ev.status.clone(), ev.message.clone(), ev.last_synced))
                {
                    state.emit(ev.clone());
                }
            }
            last = rows
                .into_iter()
                .map(|ev| (ev.name, (ev.status, ev.message, ev.last_synced)))
                .collect();
            primed = true;
            thread::sleep(Duration::from_millis(800));
        }
    });
}

/// 按范围选择参与同步的仓库（与界面范围下拉语义一致）：
/// days <= 0 为全部已配置仓库；days > 0 为最近 N 天未同步（含从未同步）
pub fn select_stale_names(state: &AppState, days: i64) -> Vec<String> {
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let scanned = scan::scan_all(Path::new(&base_dir));
    let conn = lock(&state.conn);
    let states = crate::state::load_repo_states(&conn);
    let cutoff = now_ms() - days.saturating_mul(86_400_000);
    scanned
        .into_iter()
        .filter(|r| !r.source.is_empty() && !r.targets.is_empty())
        .filter(|r| {
            if days <= 0 {
                return true;
            }
            match states.get(&r.name).and_then(|s| s.last_synced) {
                Some(t) => t < cutoff,
                None => true,
            }
        })
        .map(|r| r.name)
        .collect()
}

fn set_running(state: &AppState, repo_name: &str) {
    let conn = lock(&state.conn);
    let _ = conn.execute(
        "INSERT INTO repo_state (name, last_status) VALUES (?1, 'running')
         ON CONFLICT(name) DO UPDATE SET last_status = 'running', last_message = NULL",
        params![repo_name],
    );
    let _ = conn.execute(
        "UPDATE target_state SET last_status = 'running', last_message = NULL WHERE repo_name = ?1",
        params![repo_name],
    );
}

fn finish(
    state: &AppState,
    repo_name: &str,
    status: &str,
    message: Option<String>,
    update_time: bool,
    operator: &str,
    lang: Lang,
) {
    {
        let conn = lock(&state.conn);
        // 状态行按目录名写入（首次同步时行尚不存在）
        if update_time {
            let _ = conn.execute(
                "INSERT INTO repo_state (name, last_status, last_message, last_synced)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO UPDATE SET
                   last_status = excluded.last_status,
                   last_message = excluded.last_message,
                   last_synced = excluded.last_synced",
                params![repo_name, status, message, now_ms()],
            );
        } else {
            let _ = conn.execute(
                "INSERT INTO repo_state (name, last_status, last_message)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET
                   last_status = excluded.last_status,
                   last_message = excluded.last_message",
                params![repo_name, status, message],
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

fn run_sync(state: &AppState, repo_name: &str, operator: &str, lang: Lang) {
    // 配置实时扫描：源地址与备份目标以当前 `.git/config` 为准
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let Some(scanned) = scan::scan_one(Path::new(&base_dir), repo_name) else {
        return; // 物理仓库已不存在：无从同步
    };
    let last_synced = {
        let conn = lock(&state.conn);
        crate::state::load_repo_states(&conn)
            .get(repo_name)
            .and_then(|s| s.last_synced)
    };
    let targets = scanned.targets.clone();

    set_running(state, repo_name);
    emit_status(
        state,
        SyncEvent {
            name: repo_name.to_string(),
            status: "running".into(),
            message: None,
            last_synced,
        },
    );

    if scanned.source.is_empty() {
        let msg = tr(lang, "sync-no-source");
        finish(
            state,
            repo_name,
            "failed",
            Some(msg.clone()),
            false,
            operator,
            lang,
        );
        emit_status(
            state,
            SyncEvent {
                name: repo_name.to_string(),
                status: "failed".into(),
                message: Some(msg),
                last_synced,
            },
        );
        return;
    }
    if targets.is_empty() {
        let msg = tr(lang, "sync-no-targets");
        finish(
            state,
            repo_name,
            "failed",
            Some(msg.clone()),
            false,
            operator,
            lang,
        );
        emit_status(
            state,
            SyncEvent {
                name: repo_name.to_string(),
                status: "failed".into(),
                message: Some(msg),
                last_synced,
            },
        );
        return;
    }

    state.add_log(
        &tr_a(lang, "log-sync-started", &[("name", repo_name)]),
        operator,
    );
    let (status, message) = perform_git_sync(
        state,
        repo_name,
        &scanned.source,
        &targets,
        Path::new(&base_dir),
        lang,
    );
    let success = status == "success";
    finish(
        state,
        repo_name,
        &status,
        Some(message.clone()),
        success,
        operator,
        lang,
    );
    // 未执行到推送的目标行（如克隆/拉取阶段失败）从 running 复位
    crate::git::reset_running_targets(state, repo_name);
    emit_status(
        state,
        SyncEvent {
            name: repo_name.to_string(),
            status,
            message: Some(message),
            last_synced: if success { Some(now_ms()) } else { last_synced },
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::time::Instant;

    /// select_stale_names 与界面范围语义一致：0=全部已配置，N=最近 N 天未同步（含从未同步）；
    /// 未配置（缺源/缺目标）与物理目录已消失的仓库不参与
    #[test]
    fn select_stale_names_matches_range_semantics() {
        let root = std::env::temp_dir().join(format!("grs-stale-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        // 物理仓库：fresh / stale / never 已配置；unconf 无 origin；notarget 无备份目标
        let mk = |name: &str, config: &str| {
            let d = base.join(name);
            std::fs::create_dir_all(d.join(".git")).unwrap();
            std::fs::write(d.join(".git/config"), config).unwrap();
        };
        let both = |origin: &str, backup: &str| {
            format!("[remote \"origin\"]\n\turl = {origin}\n[remote \"backup\"]\n\turl = {backup}\n")
        };
        mk("fresh", &both("https://src/f.git", "https://bak/f.git"));
        mk("stale", &both("https://src/s.git", "https://bak/s.git"));
        mk("never", &both("https://src/n.git", "https://bak/n.git"));
        mk("unconf", "[core]\n\tbare = false\n");
        mk("notarget", "[remote \"origin\"]\n\turl = https://src/t.git\n");

        let state = AppState::open(root.join("app.db")).unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());
        // 预置同步状态
        {
            let conn = lock(&state.conn);
            let now = now_ms();
            let day: i64 = 86_400_000;
            conn.execute(
                "INSERT INTO repo_state (name, last_synced) VALUES ('fresh', ?1)",
                params![now],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO repo_state (name, last_synced) VALUES ('stale', ?1)",
                params![now - 10 * day],
            )
            .unwrap();
        }
        assert_eq!(
            select_stale_names(&state, 0),
            vec!["fresh", "never", "stale"],
            "全部范围 = 已配置仓库（按名称排序）"
        );
        assert_eq!(
            select_stale_names(&state, 7),
            vec!["never", "stale"],
            "7 天范围 = 从未同步 + 超期仓库"
        );
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 在基地址下建一个带指定远端的仓库目录（模拟用户自行克隆并配置好远端：
    /// 软件不代管远端，中转目录由用户准备）
    fn mk_repo(base: &Path, name: &str, config: &str) {
        let d = base.join(name);
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join(".git/config"), config).unwrap();
    }

    /// 仓库级同步状态（repo_state 表）
    fn repo_status(state: &AppState, name: &str) -> String {
        let conn = lock(&state.conn);
        conn.query_row(
            "SELECT last_status FROM repo_state WHERE name = ?1",
            params![name],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_default()
    }

    /// enqueue_syncs：未配置仓库跳过且不投递任务
    #[test]
    fn enqueue_syncs_skips_unconfigured() {
        let root = std::env::temp_dir().join(format!("grs-enq-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        // r1 目录存在但 config 里没有 origin（未配置）；nope 目录根本不存在
        mk_repo(&base, "r1", "[core]\n\tbare = false\n");
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        state.set_setting("base_dir", &base.to_string_lossy());
        let names = ["r1".to_string(), "nope".to_string()];
        let results = enqueue_syncs(&state, &names, "test", crate::lang::Lang::Zh);
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

    /// enqueue_syncs：物理仓库已删除（同步状态仍在库里）时跳过，
    /// 不会被重新 clone 回来
    #[test]
    fn enqueue_syncs_skips_vanished_repo() {
        let root =
            std::env::temp_dir().join(format!("grs-enq-gone-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        state.set_setting("base_dir", &base.to_string_lossy());
        {
            let conn = lock(&state.conn);
            // 曾经同步过的仓库：状态仍在库里，物理目录已被删除
            conn.execute(
                "INSERT INTO repo_state (name, last_status, last_synced) VALUES ('r1', 'success', 1)",
                [],
            )
            .unwrap();
        }
        let names = ["r1".to_string()];
        let results = enqueue_syncs(&state, &names, "test", crate::lang::Lang::Zh);
        assert_eq!(
            results,
            vec![("r1".to_string(), false)],
            "物理仓库已消失，不应启动同步"
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
            "INSERT INTO sync_jobs (repo_name, operator, lang, state, created_at)
             VALUES ('r1', 'other', 'zh', 'queued', 0)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO sync_jobs (repo_name, operator, lang, state, created_at)
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
                "INSERT INTO sync_jobs (repo_name, operator, lang, state, created_at)
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
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        // 仓库存在但没有 origin（缺源地址）
        mk_repo(&base, "demo", "[core]\n\tbare = false\n");
        let state = AppState::open(root.join("app.db")).unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());
        run_sync(&state, "demo", "test", crate::lang::Lang::Zh);

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
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        // 用户自行克隆到中转目录并添加备份远端（软件不代管远端与克隆）
        git(
            &["clone", "-q", source.to_str().unwrap(), base.join("demo").to_str().unwrap()],
            &root,
        );
        git(&["remote", "add", "backup", target.to_str().unwrap()], &base.join("demo"));
        state.set_setting("base_dir", &base.to_string_lossy());
        assert!(state.try_become_daemon());
        let results = enqueue_syncs(&state, &["demo".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(results, vec![("demo".to_string(), true)]);

        // 轮询：任务被消费、状态成功
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (jobs, status): (i64, String) = {
                let conn = lock(&state.conn);
                let jobs: i64 = conn
                    .query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                    .unwrap_or(-1);
                let status: String = conn
                    .query_row("SELECT last_status FROM repo_state WHERE name = 'demo'", [], |r| {
                        r.get(0)
                    })
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
        assert!(!state.stop_requested("demo"));
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
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let s = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(s.success(), "git {:?} 失败", args);
        };
        git(&["init", "-q", "-b", "main"], &repo_dir);
        git(&["config", "user.name", "t"], &repo_dir);
        git(&["config", "user.email", "t@t"], &repo_dir);
        std::fs::write(repo_dir.join("a.txt"), "v").unwrap();
        git(&["add", "."], &repo_dir);
        git(&["commit", "-q", "-m", "v"], &repo_dir);
        // 用户自行克隆到中转目录并添加备份远端（hook 尚未安装，克隆不会挂起）
        git(
            &[
                "clone",
                "-q",
                repo_dir.to_str().unwrap(),
                base.join("busy").to_str().unwrap(),
            ],
            &root,
        );
        git(
            &["remote", "add", "backup", repo_dir.join("b.git").to_str().unwrap()],
            &base.join("busy"),
        );
        // r2 已配置（仅 config，无真实 git）：worker 忙于 busy 时排队
        mk_repo(
            &base,
            "r2",
            "[remote \"origin\"]\n\turl = https://src/r2.git\n\
             [remote \"backup\"]\n\turl = https://bak/r2.git\n",
        );
        // 源仓库装 pre-auto-gc hook：fetch 时源仓库的 upload-pack 触发
        // auto-gc 而挂起
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

        state.set_setting("base_dir", &base.to_string_lossy());
        // 竞选执行者并投递：busy 运行（挂起）、r2 排队
        assert!(state.try_become_daemon());
        let results = enqueue_syncs(&state, &["busy".to_string(), "r2".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(
            results,
            vec![("busy".to_string(), true), ("r2".to_string(), true)]
        );
        // 重复投递被唯一索引拒绝
        let dup = enqueue_syncs(&state, &["busy".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(dup, vec![("busy".to_string(), false)], "运行中的仓库不应重复投递");

        // 等 busy 进入运行态后再等一个轮询周期，确保 git 已挂起
        let mut waited = 0;
        loop {
            let status = repo_status(&state, "busy");
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

        // 停止全部：busy 标记停止（跨进程语义）、r2 排队出队
        stop_sync(st.clone(), "tok".into(), None).unwrap();
        let mut status = String::new();
        for _ in 0..200 {
            status = repo_status(&state, "busy");
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
        assert!(!state.stop_requested("busy"), "停止标记应已清理");
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
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let s = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(s.success(), "git {:?} 失败", args);
        };
        git(&["init", "-q", "-b", "main"], &source_repo);
        git(&["config", "user.name", "t"], &source_repo);
        git(&["config", "user.email", "t@t"], &source_repo);
        std::fs::write(source_repo.join("a.txt"), "v").unwrap();
        git(&["add", "."], &source_repo);
        git(&["commit", "-q", "-m", "v"], &source_repo);
        // 用户自行克隆到中转目录并添加备份远端（hook 尚未安装，克隆不会挂起）
        git(
            &[
                "clone",
                "-q",
                source_repo.to_str().unwrap(),
                base.join("busy").to_str().unwrap(),
            ],
            &root,
        );
        git(
            &["remote", "add", "backup", source_repo.join("b.git").to_str().unwrap()],
            &base.join("busy"),
        );
        // 源仓库装 pre-auto-gc hook：fetch 时源仓库的 upload-pack 触发
        // auto-gc 而挂起
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
        state.set_setting("base_dir", &base.to_string_lossy());
        assert!(state.try_become_daemon());
        let results = enqueue_syncs(&state, &["busy".to_string()], "test", crate::lang::Lang::Zh);
        assert_eq!(results, vec![("busy".to_string(), true)]);
        let mut waited = 0;
        loop {
            let status = repo_status(&state, "busy");
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
            conn.execute("INSERT OR IGNORE INTO sync_stop (repo_name) VALUES ('busy')", [])
                .unwrap();
        }
        let mut status = String::new();
        for _ in 0..200 {
            status = repo_status(&state, "busy");
            if status == "stopped" && !state.stop_requested("busy") {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(status, "stopped", "他方写入的停止标记应终止同步");
        assert!(!state.stop_requested("busy"), "停止标记应已清理");
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
                    "INSERT INTO repo_state (name, last_status) VALUES ('r1', 'running')",
                    [],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO sync_jobs (repo_name, operator, lang, state, created_at)
                     VALUES ('r1', 'test', 'zh', 'running', 0)",
                    [],
                )
                .unwrap();
                conn.execute("INSERT INTO sync_stop (repo_name) VALUES ('r1')", []).unwrap();
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
                .query_row("SELECT last_status FROM repo_state WHERE name = 'r1'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            (jobs, flags, status)
        };
        assert_eq!(jobs, 0, "残留 running 任务应被复位");
        assert_eq!(flags, 0, "残留停止标记应被清理");
        assert_eq!(status, "idle", "running 仓库状态应复位");
        drop(b);
        std::fs::remove_dir_all(&root).ok();
    }

    /// start_sync 命令层：会话校验、未配置仓库跳过、返回实际投递数；
    /// 任务被消费后仓库状态落为 failed（源指向缺失的本地路径，克隆立即
    /// 失败且不触网）
    #[test]
    fn start_sync_command_counts_and_guards() {
        use crate::state::testutil::{insert_session, open_mock_app};
        use tauri::Manager;

        let (app, state, root) = open_mock_app("startcmd");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        // r1 已配置（origin 指向缺失的本地路径，fetch 立即失败且不触网）；
        // r2 未配置（config 里没有 origin）
        let missing = root.join("no-such-repo").to_string_lossy().to_string();
        mk_repo(
            &base,
            "r1",
            &format!(
                "[remote \"origin\"]\n\turl = {missing}\n\
                 [remote \"backup\"]\n\turl = https://bak/r1.git\n"
            ),
        );
        mk_repo(&base, "r2", "[core]\n\tbare = false\n");
        state.set_setting("base_dir", &base.to_string_lossy());
        // 无效令牌拒绝
        assert!(start_sync(st.clone(), "bad".into(), vec!["r1".into()]).is_err());
        // 返回实际投递数：未配置的 r2 跳过
        let n = start_sync(
            st.clone(),
            "tok".into(),
            vec!["r1".into(), "r2".into()],
        )
        .unwrap();
        assert_eq!(n, 1, "只有已配置的 r1 投递");

        // 轮询：任务被消费、拉取失败落为 failed
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (jobs, status): (i64, String) = {
                let conn = lock(&state.conn);
                let jobs: i64 = conn
                    .query_row("SELECT COUNT(*) FROM sync_jobs", [], |r| r.get(0))
                    .unwrap_or(-1);
                let status: String = conn
                    .query_row("SELECT last_status FROM repo_state WHERE name = 'r1'", [], |r| {
                        r.get(0)
                    })
                    .unwrap_or_default();
                (jobs, status)
            };
            if jobs == 0 && status == "failed" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "任务未被消费: jobs={jobs} status={status}"
            );
            thread::sleep(Duration::from_millis(100));
        }
        drop(st);
        drop(app);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// wait_syncs_idle：客户端进程立即返回（即使表中残留 running 行，
    /// 那是执行者的事）；执行者在无 running 任务时也立即返回
    #[test]
    fn wait_syncs_idle_returns_for_client_and_idle_daemon() {
        let root = std::env::temp_dir().join(format!("grs-idle-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO sync_jobs (repo_name, operator, lang, state, created_at)
                 VALUES ('r1', 'test', 'zh', 'running', 0)",
                [],
            )
            .unwrap();
        }
        // 客户端（未持有守护角色）：立即返回
        state.wait_syncs_idle();
        // 执行者 + 无 running 任务：立即返回
        {
            let conn = lock(&state.conn);
            conn.execute("DELETE FROM sync_jobs", []).unwrap();
        }
        assert!(state.try_become_daemon());
        state.wait_syncs_idle();
        drop(state);
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
                "INSERT INTO repo_state (name, last_status) VALUES ('demo', 'idle')",
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
                "UPDATE repo_state SET last_status = 'running' WHERE name = 'demo'",
                [],
            )
            .unwrap();
        }
        let ev = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("应收到合成的状态事件");
        assert_eq!(ev.name, "demo");
        assert_eq!(ev.status, "running");
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }
}
