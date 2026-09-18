use crate::auth::require_session;
use crate::lang::{gui_lang, tr, tr_a, Lang};
use crate::state::{lock, now_ms, AppState, Repo, SyncHandle};
use rusqlite::params;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

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

const REPO_COLS: &str =
    "id, name, source, target, last_synced, last_status, last_message";

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
    let handles: Vec<(String, Arc<SyncHandle>)> = {
        let procs = lock(&state.sync_procs);
        match &id {
            Some(i) => procs
                .get(i)
                .map(|h| (i.clone(), h.clone()))
                .into_iter()
                .collect(),
            None => procs.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        }
    };
    if handles.is_empty() {
        return Ok(());
    }
    {
        let mut stop = lock(&state.stop_requested);
        for (rid, _) in &handles {
            stop.insert(rid.clone());
        }
    }
    for (_, h) in handles {
        let mut slot = lock(&h.child);
        if let Some(c) = slot.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        slot.take();
    }
    state.add_log("停止同步", &username);
    Ok(())
}

/// 启动后台同步线程；返回 false 表示该仓库已在同步中
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
    let st = state.clone();
    thread::spawn(move || {
        run_sync(&st, &app, &repo_id, &operator, lang);
        lock(&st.syncing).remove(&repo_id);
        lock(&st.stop_requested).remove(&repo_id);
    });
    true
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
}

/// 同步流水线（AGENTS.md 软件逻辑）：
/// 1. 从源仓库拉取到本地基地址下的工作副本（中转站）
/// 2. 更新中转站的 LFS 与 submodule
/// 3. 推送到目标仓库地址
fn perform_git_sync(
    state: &AppState,
    repo_id: &str,
    repo: &Repo,
    base_dir: &Path,
    lang: Lang,
) -> (String, String) {
    let local = base_dir.join(&repo.name);
    let mut done: Vec<String> = Vec::new();
    let mut steps: Vec<GitStep> = Vec::new();

    if local.exists() {
        steps.push(GitStep {
            args: vec![
                "remote".into(),
                "set-url".into(),
                "origin".into(),
                repo.source.clone(),
            ],
            cwd: Some(local.clone()),
            ok_msg: String::new(),
        });
        steps.push(GitStep {
            args: vec![
                "fetch".into(),
                "origin".into(),
                "--prune".into(),
                "--tags".into(),
            ],
            cwd: Some(local.clone()),
            ok_msg: tr(lang, "step-fetch"),
        });
        steps.push(GitStep {
            args: vec!["pull".into(), "--ff-only".into()],
            cwd: Some(local.clone()),
            ok_msg: String::new(),
        });
    } else {
        if let Some(parent) = local.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        steps.push(GitStep {
            args: vec![
                "clone".into(),
                repo.source.clone(),
                local.to_string_lossy().to_string(),
            ],
            cwd: None,
            ok_msg: tr(lang, "step-clone"),
        });
    }

    // LFS：git-lfs 未安装时跳过并在结果中注明
    let lfs_available = {
        let mut cmd = Command::new("git");
        cmd.args(["lfs", "version"]);
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        cmd.status().map(|s| s.success()).unwrap_or(false)
    };
    if lfs_available {
        steps.push(GitStep {
            args: vec!["lfs".into(), "pull".into()],
            cwd: Some(local.clone()),
            ok_msg: tr(lang, "step-lfs"),
        });
    }

    steps.push(GitStep {
        args: vec![
            "submodule".into(),
            "update".into(),
            "--init".into(),
            "--recursive".into(),
        ],
        cwd: Some(local.clone()),
        ok_msg: tr(lang, "step-submodule"),
    });
    steps.push(GitStep {
        args: vec![
            "push".into(),
            repo.target.clone(),
            "+refs/heads/*:refs/heads/*".into(),
            "+refs/tags/*:refs/tags/*".into(),
        ],
        cwd: Some(local.clone()),
        ok_msg: tr(lang, "step-push"),
    });

    for step in &steps {
        // “停止同步”请求：kill 之后的剩余步骤不再执行
        if lock(&state.stop_requested).contains(repo_id) {
            return ("stopped".into(), tr(lang, "manually-stopped"));
        }
        match run_git(state, repo_id, &step.args, step.cwd.as_deref(), lang) {
            Err(e) => {
                if lock(&state.stop_requested).contains(repo_id) {
                    return ("stopped".into(), tr(lang, "manually-stopped"));
                }
                // 仅 git-lfs 未安装允许跳过
                if step.args.first().map(String::as_str) == Some("lfs")
                    && e.contains("is not a git command")
                {
                    done.push(tr(lang, "lfs-skipped"));
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

    let lfs_note = if !lfs_available {
        format!("{}{}", lang.sep(), tr(lang, "lfs-skipped"))
    } else {
        String::new()
    };
    (
        "success".into(),
        format!("{}{lfs_note}", done.join(lang.sep())),
    )
}

/// 运行 git 命令：子进程注册到 sync_procs 以支持“停止同步”。
/// 成功返回 stderr/stdout 合并文本（可能为空），失败返回错误文本。
fn run_git(
    state: &AppState,
    repo_id: &str,
    args: &[String],
    cwd: Option<&Path>,
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

    // stderr 交给独立线程收集，避免管道写满阻塞
    let stderr = child.stderr.take();
    let stdout_pipe = child.stdout.take();
    let reader = thread::spawn(move || -> String {
        let mut buf = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut buf);
        }
        buf
    });
    let handle = Arc::new(SyncHandle {
        child: Mutex::new(Some(child)),
    });
    lock(&state.sync_procs).insert(repo_id.to_string(), handle.clone());

    let mut stdout_buf = String::new();
    if let Some(mut out) = stdout_pipe {
        let _ = out.read_to_string(&mut stdout_buf);
    }
    let mut status = None;
    let mut killed = false;
    {
        let mut slot = lock(&handle.child);
        if let Some(c) = slot.as_mut() {
            status = c.wait().ok();
        } else {
            killed = true;
        }
    }
    lock(&state.sync_procs).remove(repo_id);
    let mut text = reader.join().unwrap_or_default();
    if !stdout_buf.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&stdout_buf);
    }
    let text = text.trim().to_string();

    if killed {
        return Err(tr(lang, "process-terminated"));
    }
    match status {
        Some(s) if s.success() => Ok(text),
        Some(_) => {
            eprintln!("git {:?} 执行失败：{text}", args.first());
            Err(if text.is_empty() {
                format!("git {:?} 执行失败", args.first())
            } else {
                text
            })
        }
        None => Err(tr(lang, "git-wait-error")),
    }
}
