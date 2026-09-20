//! 同步任务：Tauri 命令（开始/停止）、串行队列与单个仓库的同步执行编排。
//! git 子进程细节见 git.rs；仓库 CRUD 见 repos.rs。

use crate::auth::require_session;
use crate::git::perform_git_sync;
use crate::lang::{gui_lang, tr, tr_a, Lang};
use crate::state::{lock, now_ms, repo_from_row, AppState, SyncJob, REPO_COLS};
use rusqlite::params;
use std::path::Path;
use std::thread;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub id: String,
    pub status: String,
    pub message: Option<String>,
    pub last_synced: Option<i64>,
}

fn emit_status(app: &Option<AppHandle>, ev: SyncEvent) {
    if let Some(a) = app {
        let _ = a.emit("sync-status", ev);
    }
}

#[tauri::command]
pub fn start_sync(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    token: String,
    ids: Vec<String>,
) -> Result<usize, String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let results = enqueue_syncs(state.inner(), Some(app), &ids, &username, lang);
    Ok(results.iter().filter(|(_, started)| *started).count())
}

#[tauri::command]
pub fn stop_sync(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: Option<String>,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    // 目标集合：指定 id 或全部（含排队中与运行中）
    let ids: Vec<String> = {
        let syncing = lock(&state.syncing);
        match &id {
            Some(i) => syncing
                .contains(i)
                .then(|| i.clone())
                .into_iter()
                .collect(),
            None => syncing.iter().cloned().collect(),
        }
    };
    if ids.is_empty() {
        return Ok(());
    }

    // 排队中（尚未开始运行）的任务直接出队
    {
        let mut q = lock(&state.sync_queue);
        q.jobs.retain(|j| !ids.contains(&j.repo_id));
    }
    for rid in &ids {
        lock(&state.syncing).remove(rid);
    }

    // 运行中的任务终止其 git 子进程
    let mut stopped = false;
    for rid in &ids {
        let handle = lock(&state.sync_procs).get(rid).cloned();
        if let Some(h) = handle {
            let mut slot = lock(&h.child);
            if let Some(c) = slot.as_mut() {
                let _ = c.kill();
                let _ = c.wait();
            }
            slot.take();
            lock(&state.stop_requested).insert(rid.clone());
            stopped = true;
        } else {
            stopped = true;
        }
    }
    if stopped {
        state.add_log(&tr(gui_lang(), "log-sync-stopped-cmd"), &username);
    }
    Ok(())
}

/// 批量入队同步（界面与 MCP 共用）：未配置（缺源地址或备份目标）的仓库跳过，
/// 已在同步/排队的仓库不重复入队；按入参顺序返回每个仓库是否实际启动
pub fn enqueue_syncs(
    state: &Arc<AppState>,
    app: Option<AppHandle>,
    ids: &[String],
    operator: &str,
    lang: Lang,
) -> Vec<(String, bool)> {
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
        let started = spawn_sync(state.clone(), app.clone(), id.clone(), operator.to_string(), lang);
        results.push((id.clone(), started));
    }
    results
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

/// 入队一个同步任务；队列由单一 worker 串行消费，返回 false 表示该仓库已在队列或同步中
pub fn spawn_sync(
    state: Arc<AppState>,
    app: Option<AppHandle>,
    repo_id: String,
    operator: String,
    lang: Lang,
) -> bool {
    {
        let mut syncing = lock(&state.syncing);
        if syncing.contains(&repo_id) {
            return false;
        }
        syncing.insert(repo_id.clone());
    }
    lock(&state.stop_requested).remove(&repo_id);
    let should_spawn = {
        let mut q = lock(&state.sync_queue);
        q.jobs.push_back(SyncJob {
            repo_id: repo_id.clone(),
            operator,
            lang,
        });
        if !q.worker_active && !q.jobs.is_empty() {
            q.worker_active = true;
            true
        } else {
            false
        }
    };
    if should_spawn {
        let st = state.clone();
        thread::spawn(move || sync_worker(&st, app));
    }
    true
}

/// 串行消费同步队列：同一时间只运行一个仓库的同步
fn sync_worker(state: &Arc<AppState>, app: Option<AppHandle>) {
    loop {
        let job = lock(&state.sync_queue).jobs.pop_front();
        match job {
            Some(job) => {
                run_sync(state, &app, &job.repo_id, &job.operator, job.lang);
                lock(&state.syncing).remove(&job.repo_id);
                lock(&state.stop_requested).remove(&job.repo_id);
            }
            None => {
                // 队列已空；与入队方竞态时双重检查，避免漏掉新任务
                let mut q = lock(&state.sync_queue);
                if q.jobs.is_empty() {
                    q.worker_active = false;
                    return;
                }
                // 有新任务入队，继续消费
            }
        }
    }
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

fn run_sync(
    state: &AppState,
    app: &Option<AppHandle>,
    repo_id: &str,
    operator: &str,
    lang: Lang,
) {
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
        app,
        SyncEvent {
            id: repo_id.to_string(),
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
            app,
            SyncEvent {
                id: repo_id.to_string(),
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
            app,
            SyncEvent {
                id: repo_id.to_string(),
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
        app,
        SyncEvent {
            id: repo_id.to_string(),
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

    /// enqueue_syncs：未配置仓库跳过且不启动同步（不产生 git 子进程）
    #[test]
    fn enqueue_syncs_skips_unconfigured_without_spawning() {
        let root = std::env::temp_dir().join(format!("grs-enq-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        {
            let conn = lock(&state.conn);
            conn.execute("INSERT INTO repos (id, name, source) VALUES ('r1', 'r1', '')", [])
                .unwrap();
        }
        let ids = ["r1".to_string(), "nope".to_string()];
        let results = enqueue_syncs(&state, None, &ids, "test", crate::lang::Lang::Zh);
        assert_eq!(
            results,
            vec![("r1".to_string(), false), ("nope".to_string(), false)]
        );
        assert!(lock(&state.syncing).is_empty(), "未产生任何同步任务");
        assert!(lock(&state.sync_queue).jobs.is_empty());
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 失败日志附带原因：缺源地址的仓库走完整 run_sync 后，
    /// 操作日志应记录包含可读原因（sync-no-source 文案）的失败条目
    #[test]
    fn failed_sync_logs_reason() {
        use crate::lang::tr;
        use crate::state::testutil::open_mock_app;

        let (app, state, root) = open_mock_app("faillog");
        {
            let conn = lock(&state.conn);
            conn.execute("INSERT INTO repos (id, name, source) VALUES ('r1', 'demo', '')", [])
                .unwrap();
        }
        run_sync(state.as_ref(), &None, "r1", "test", crate::lang::Lang::Zh);

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

        drop(app);
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

    /// 命令层「停止同步」：重复入队不产生双任务；停止后 syncing 清空、
    /// git 子进程被终止、repos 状态为 stopped
    #[test]
    fn stop_sync_kills_running_process_and_clears_state() {
        use crate::state::testutil::{insert_session, open_mock_app};
        use std::io::Write;
        use tauri::Manager;

        let (app, state, root) = open_mock_app("stop");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");

        // 仓库源指向一个 git 子命令：读到 stdin 关闭才退出，保证子进程存活可被终止
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
        }
        // 源目录准备成 git 仓库并写入一个挂起的 hook：git 命令会卡在 hook 上
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
        // pre-auto-gc hook：读 stdin 直到 EOF，模拟长时间挂起的 git 操作
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

        // 入队两次同一仓库：第二次因已在 syncing 被拒
        let started = spawn_sync(state.clone(), None, "r1".into(), "test".into(), crate::lang::Lang::Zh);
        assert!(started);
        let dup = spawn_sync(state.clone(), None, "r1".into(), "test".into(), crate::lang::Lang::Zh);
        assert!(!dup, "同步中的仓库不应重复入队");

        // 等 worker 启动 git 子进程（轮询 sync_procs 出现）；并行测试下 CPU 紧张，
        // 出现后再等一个轮询周期，确保 git 已进入被轮询等待的运行态
        let mut waited = 0;
        while lock(&state.sync_procs).is_empty() {
            if waited > 200 {
                panic!("同步子进程未启动");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            waited += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(600));

        stop_sync(st.clone(), "tok".into(), None).unwrap();
        // 终止请求后 worker 收尾（杀进程 → 落库）：轮询至 syncing 清空再断言
        let mut status = String::new();
        for _ in 0..200 {
            status = {
                let conn = lock(&state.conn);
                conn.query_row("SELECT last_status FROM repos WHERE id = 'r1'", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap_or_default()
            };
            if status == "stopped" && lock(&state.syncing).is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(status, "stopped", "停止后仓库状态应为 stopped");
        drop(st);
        drop(app);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }
}
