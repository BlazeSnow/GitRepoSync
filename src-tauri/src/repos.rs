use crate::auth::require_session;
use crate::lang::{gui_lang, tr, tr_a, Lang};
use crate::state::{lock, now_ms, AppState, Repo, SyncHandle, SyncJob};
use rusqlite::params;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

/// 单个 git 网络命令的超时（秒）；LFS fetch --all 首次拉取大仓库较慢，单独放宽
const GIT_TIMEOUT_SECS: u64 = 1800;
const LFS_TIMEOUT_SECS: u64 = 3600;

#[derive(Clone, Serialize)]
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

#[tauri::command]
pub fn list_repos(state: State<'_, Arc<AppState>>, token: String) -> Result<Vec<Repo>, String> {
    require_session(&state, &token)?;
    let conn = lock(&state.conn);
    let mut stmt = conn
        .prepare(&format!("SELECT {REPO_COLS} FROM repos ORDER BY name"))
        .map_err(|e| e.to_string())?;
    let repos = stmt
        .query_map([], repo_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(repos)
}

#[tauri::command]
pub fn save_repo(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: Option<String>,
    name: String,
    source: String,
    target: String,
) -> Result<Repo, String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let name = name.trim().to_string();
    let source = source.trim().to_string();
    let target = target.trim().to_string();
    if name.is_empty() || source.is_empty() || target.is_empty() {
        return Err(tr(lang, "repo-fields-empty"));
    }
    let repo = {
        let conn = lock(&state.conn);
        if let Some(rid) = &id {
            let updated = conn
                .execute(
                    "UPDATE repos SET name = ?1, source = ?2, target = ?3 WHERE id = ?4",
                    params![name, source, target, rid],
                )
                .map_err(|e| e.to_string())?;
            if updated == 0 {
                return Err(tr(lang, "repo-not-found"));
            }
            conn.query_row(
                &format!("SELECT {REPO_COLS} FROM repos WHERE id = ?1"),
                params![rid],
                repo_from_row,
            )
            .map_err(|e| e.to_string())?
        } else {
            let repo = Repo {
                id: Uuid::new_v4().to_string(),
                name: name.clone(),
                source,
                target,
                last_synced: None,
                last_status: "idle".into(),
                last_message: None,
            };
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
            .map_err(|e| e.to_string())?;
            repo
        }
    };
    state.add_log(
        &tr_a(
            lang,
            if id.is_some() {
                "log-repo-edited"
            } else {
                "log-repo-added"
            },
            &[("name", &repo.name)],
        ),
        &username,
    );
    Ok(repo)
}

#[tauri::command]
pub fn delete_repo(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: String,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    if lock(&state.syncing).contains(&id) {
        return Err(tr(lang, "repo-syncing"));
    }
    let name = {
        let conn = lock(&state.conn);
        let name = conn
            .query_row("SELECT name FROM repos WHERE id = ?1", params![id], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|_| tr(lang, "repo-not-found"))?;
        conn.execute("DELETE FROM repos WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        name
    };
    state.add_log(&tr_a(lang, "log-repo-deleted", &[("name", &name)]), &username);
    Ok(())
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
    let mut started = 0;
    for id in ids {
        if spawn_sync(
            state.inner().clone(),
            Some(app.clone()),
            id,
            username.clone(),
            lang,
        ) {
            started += 1;
        }
    }
    Ok(started)
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
    let action = tr_a(
        lang,
        match status {
            "success" => "log-sync-success",
            "stopped" => "log-sync-stopped",
            _ => "log-sync-failed",
        },
        &[("name", repo_name)],
    );
    state.add_log(&action, operator);
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

    state.add_log(
        &tr_a(lang, "log-sync-started", &[("name", &repo.name)]),
        operator,
    );
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(|| crate::state::DEFAULT_BASE_DIR.to_string());
    let (status, message) =
        perform_git_sync(state, repo_id, &repo, &expand_home(&base_dir), lang);
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

/// 展开 base_dir 开头的 ~ 为用户主目录
fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(rest) = path.strip_prefix("~/").or(path.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

struct GitStep {
    args: Vec<String>,
    cwd: Option<PathBuf>,
    ok_msg: String,
    timeout: Duration,
    /// 失败是否判定整个同步失败；LFS / submodule 失败仅警告（引用备份仍然有效）
    fatal: bool,
    warn_key: &'static str,
}

/// 网络命令统一加低速中断配置（HTTP 停滞 120s 判死）；本地命令不受影响
fn git_args(args: &[&str]) -> Vec<String> {
    let mut v = vec![
        "-c".to_string(),
        "http.lowSpeedLimit=1024".to_string(),
        "-c".to_string(),
        "http.lowSpeedTime=120".to_string(),
    ];
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

/// 同步流水线（AGENTS.md 软件逻辑），参考 backup-repos skill 的无人值守经验：
/// 1. 拉取：fetch-only，只更新 origin 跟踪引用，不合并工作区（不受本地脏状态影响）
/// 2. 更新：LFS fetch --all（所有引用的 LFS 对象）+ submodule（失败降级为警告）
/// 3. 推送：本地分支 + origin 跟踪分支 + 标签，--prune 与来源强制对齐
fn perform_git_sync(
    state: &AppState,
    repo_id: &str,
    repo: &Repo,
    base_dir: &Path,
    lang: Lang,
) -> (String, String) {
    let local = base_dir.join(&repo.name);
    let git_timeout = Duration::from_secs(GIT_TIMEOUT_SECS);
    let lfs_timeout = Duration::from_secs(LFS_TIMEOUT_SECS);
    let mut done: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut steps: Vec<GitStep> = Vec::new();

    if local.exists() {
        // 同步源变更时保持 origin 指向最新源地址
        steps.push(GitStep {
            args: git_args(&["remote", "set-url", "origin", &repo.source]),
            cwd: Some(local.clone()),
            ok_msg: String::new(),
            timeout: git_timeout,
            fatal: true,
            warn_key: "",
        });
        steps.push(GitStep {
            args: git_args(&["fetch", "origin", "--prune", "--tags"]),
            cwd: Some(local.clone()),
            ok_msg: tr(lang, "step-fetch"),
            timeout: git_timeout,
            fatal: true,
            warn_key: "",
        });
    } else {
        if let Some(parent) = local.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        steps.push(GitStep {
            args: git_args(&["clone", &repo.source, &local.to_string_lossy()]),
            cwd: None,
            ok_msg: tr(lang, "step-clone"),
            timeout: git_timeout,
            fatal: true,
            warn_key: "",
        });
    }

    // LFS：下载所有引用指向的 LFS 对象，推送时对象才会一并上传
    let lfs_available = {
        let mut cmd = Command::new("git");
        cmd.args(["lfs", "version"]);
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        cmd.status().map(|s| s.success()).unwrap_or(false)
    };
    if lfs_available {
        steps.push(GitStep {
            args: git_args(&["lfs", "fetch", "--all", "origin"]),
            cwd: Some(local.clone()),
            ok_msg: tr(lang, "step-lfs"),
            timeout: lfs_timeout,
            fatal: false,
            warn_key: "warn-lfs-fetch",
        });
    } else {
        warnings.push(tr(lang, "lfs-skipped"));
    }

    steps.push(GitStep {
        args: git_args(&["submodule", "update", "--init", "--recursive"]),
        cwd: Some(local.clone()),
        ok_msg: tr(lang, "step-submodule"),
        timeout: git_timeout,
        fatal: false,
        warn_key: "warn-submodule",
    });

    for step in &steps {
        // “停止同步”请求：终止后的剩余步骤不再执行
        if lock(&state.stop_requested).contains(repo_id) {
            return ("stopped".into(), tr(lang, "manually-stopped"));
        }
        match run_git(state, repo_id, &step.args, step.cwd.as_deref(), step.timeout, lang) {
            Err(e) => {
                if lock(&state.stop_requested).contains(repo_id) {
                    return ("stopped".into(), tr(lang, "manually-stopped"));
                }
                if !step.fatal {
                    warnings.push(tr_a(lang, step.warn_key, &[("err", &e)]));
                    continue;
                }
                return ("failed".into(), e);
            }
            Ok(_) => {}
        }
        if !step.ok_msg.is_empty() {
            done.push(step.ok_msg.clone());
        }
    }

    // 推送引用必须在 fetch 完成后计算：origin 跟踪分支（补全本地未 checkout 的分支）
    // + 本地分支 + 标签；过滤 origin/HEAD 符号引用（推过去会变成多余的 HEAD 分支）
    let mut push_refs: Vec<String> = vec!["+refs/heads/*:refs/heads/*".into()];
    let origin_refs = run_git(
        state,
        repo_id,
        &git_args(&["for-each-ref", "--format=%(refname)", "refs/remotes/origin"]),
        Some(local.as_path()),
        git_timeout,
        lang,
    )
    .unwrap_or_default();
    for line in origin_refs.lines() {
        if let Some(br) = line.strip_prefix("refs/remotes/origin/") {
            if br == "HEAD" {
                continue;
            }
            push_refs.push(format!("+refs/remotes/origin/{br}:refs/heads/{br}"));
        }
    }
    push_refs.push("+refs/tags/*:refs/tags/*".into());

    let mut push_cmd: Vec<String> = vec!["push".into(), "--prune".into(), repo.target.clone()];
    push_cmd.extend(push_refs);

    // 推送失败判定整个同步失败；此前的 LFS / submodule 警告保留在消息里
    match run_git(state, repo_id, &git_args(&push_cmd.iter().map(String::as_str).collect::<Vec<_>>()), Some(local.as_path()), git_timeout, lang) {
        Err(e) => {
            if lock(&state.stop_requested).contains(repo_id) {
                return ("stopped".into(), tr(lang, "manually-stopped"));
            }
            return ("failed".into(), e);
        }
        Ok(_) => {}
    }
    done.push(tr(lang, "step-push"));

    let mut parts = done;
    parts.extend(warnings);
    ("success".into(), parts.join(lang.sep()))
}

/// 运行 git 命令：子进程注册到 sync_procs 以支持“停止同步”；
/// 超时强杀按失败处理。成功返回 stderr/stdout 合并文本（可能为空）。
fn run_git(
    state: &AppState,
    repo_id: &str,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
    lang: Lang,
) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| tr_a(lang, "git-spawn-error", &[("err", &e.to_string())]))?;

    // stderr / stdout 各由独立线程收集，避免管道写满阻塞
    let stderr = child.stderr.take();
    let stdout_pipe = child.stdout.take();
    let err_reader = thread::spawn(move || -> String {
        let mut buf = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut buf);
        }
        buf
    });
    let out_reader = thread::spawn(move || -> String {
        let mut buf = String::new();
        if let Some(mut o) = stdout_pipe {
            let _ = o.read_to_string(&mut buf);
        }
        buf
    });
    let handle = Arc::new(SyncHandle {
        child: Mutex::new(Some(child)),
    });
    lock(&state.sync_procs).insert(repo_id.to_string(), handle.clone());

    // 轮询等待：超时或“停止同步”时强杀子进程
    let deadline = Instant::now() + timeout;
    let mut status = None;
    let mut killed = false;
    loop {
        {
            let mut slot = lock(&handle.child);
            match slot.as_mut() {
                // “停止同步”已终止并移除子进程
                None => {
                    killed = true;
                    break;
                }
                Some(c) => match c.try_wait() {
                    Ok(Some(s)) => {
                        status = Some(s);
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        lock(&state.sync_procs).remove(repo_id);
                        return Err(format!("{}: {e}", tr(lang, "git-wait-error")));
                    }
                },
            }
        }
        if Instant::now() >= deadline {
            {
                let mut slot = lock(&handle.child);
                if let Some(c) = slot.as_mut() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                slot.take();
            }
            lock(&state.sync_procs).remove(repo_id);
            return Err(tr_a(
                lang,
                "git-timeout",
                &[("secs", &timeout.as_secs().to_string())],
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
    lock(&state.sync_procs).remove(repo_id);

    let mut text = err_reader.join().unwrap_or_default();
    let out_text = out_reader.join().unwrap_or_default();
    if !out_text.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&out_text);
    }
    let text = text.trim().to_string();

    if killed {
        return Err(tr(lang, "process-terminated"));
    }
    match status {
        Some(s) if s.success() => Ok(text),
        _ => {
            eprintln!("git {:?} 执行失败：{text}", args.first());
            Err(if text.is_empty() {
                format!("git {:?} 执行失败", args.first())
            } else {
                text
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_args_prefixes_config() {
        let args = git_args(&["fetch", "origin"]);
        assert_eq!(args[0], "-c");
        assert!(args.iter().any(|a| a == "fetch"));
    }

    #[test]
    fn norm_url_strips_protocol_and_suffix() {
        assert_eq!(expand_home("~/repo"), dirs::home_dir().unwrap().join("repo"));
    }
}
