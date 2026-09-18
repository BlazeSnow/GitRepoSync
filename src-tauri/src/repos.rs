use crate::auth::require_session;
use crate::state::{lock, now_ms, AppState, Repo, SyncHandle};
use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
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

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        chars[chars.len() - n..].iter().collect()
    }
}

#[tauri::command]
pub fn list_repos(state: State<'_, Arc<AppState>>, token: String) -> Result<Vec<Repo>, String> {
    require_session(&state, &token)?;
    Ok(lock(&state.repos).clone())
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
    require_session(&state, &token)?;
    let name = name.trim().to_string();
    let source = source.trim().to_string();
    let target = target.trim().to_string();
    if name.is_empty() || source.is_empty() || target.is_empty() {
        return Err("仓库名称、源地址、目标地址均不能为空".into());
    }
    let repo = {
        let mut repos = lock(&state.repos);
        if let Some(rid) = &id {
            let r = repos
                .iter_mut()
                .find(|r| &r.id == rid)
                .ok_or("仓库不存在")?;
            r.name = name;
            r.source = source;
            r.target = target;
            r.clone()
        } else {
            let r = Repo {
                id: Uuid::new_v4().to_string(),
                name,
                source,
                target,
                last_synced: None,
                last_status: "idle".into(),
                last_message: None,
            };
            repos.push(r.clone());
            r
        }
    };
    state.save_repos()?;
    Ok(repo)
}

#[tauri::command]
pub fn delete_repo(state: State<'_, Arc<AppState>>, token: String, id: String) -> Result<(), String> {
    require_session(&state, &token)?;
    if lock(&state.syncing).contains(&id) {
        return Err("该仓库正在同步，请先停止同步".into());
    }
    {
        lock(&state.repos).retain(|r| r.id != id);
    }
    state.save_repos()?;
    Ok(())
}

#[tauri::command]
pub fn start_sync(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    token: String,
    ids: Vec<String>,
) -> Result<usize, String> {
    require_session(&state, &token)?;
    let mut started = 0;
    for id in ids {
        if spawn_sync(state.inner().clone(), Some(app.clone()), id) {
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
    require_session(&state, &token)?;
    let handles: Vec<Arc<SyncHandle>> = {
        let procs = lock(&state.sync_procs);
        match &id {
            Some(i) => procs.get(i).cloned().into_iter().collect(),
            None => procs.values().cloned().collect(),
        }
    };
    for h in handles {
        let mut slot = lock(&h.child);
        if let Some(c) = slot.as_mut() {
            let _ = c.kill();
        }
        slot.take();
    }
    Ok(())
}

/// 启动后台同步线程；返回 false 表示该仓库已在同步中
pub fn spawn_sync(state: Arc<AppState>, app: Option<AppHandle>, repo_id: String) -> bool {
    {
        let mut syncing = lock(&state.syncing);
        if syncing.contains(&repo_id) {
            return false;
        }
        syncing.insert(repo_id.clone());
    }
    let st = state.clone();
    thread::spawn(move || {
        run_sync(&st, &app, &repo_id);
        lock(&st.syncing).remove(&repo_id);
    });
    true
}

fn set_running(state: &AppState, repo_id: &str) {
    {
        let mut repos = lock(&state.repos);
        if let Some(r) = repos.iter_mut().find(|r| r.id == repo_id) {
            r.last_status = "running".into();
            r.last_message = None;
        }
    }
    let _ = state.save_repos();
}

fn finish(state: &AppState, repo_id: &str, status: &str, message: Option<String>, update_time: bool) {
    {
        let mut repos = lock(&state.repos);
        if let Some(r) = repos.iter_mut().find(|r| r.id == repo_id) {
            if update_time {
                r.last_synced = Some(now_ms());
            }
            r.last_status = status.into();
            r.last_message = message.clone();
        }
    }
    let _ = state.save_repos();
}

fn run_sync(state: &AppState, app: &Option<AppHandle>, repo_id: &str) {
    let repo = {
        let repos = lock(&state.repos);
        repos.iter().find(|r| r.id == repo_id).cloned()
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

    let (status, message) = perform_git_sync(state, repo_id, &repo);
    let success = status == "success";
    finish(state, repo_id, &status, Some(message.clone()), success);
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

/// 目标路径不存在时执行 `git clone --mirror`，否则在目标仓库内执行 `git remote update --prune`
/// 返回 (最终状态, 提示消息)，状态为 success / failed / stopped
fn perform_git_sync(state: &AppState, repo_id: &str, repo: &Repo) -> (String, String) {
    let target = PathBuf::from(&repo.target);
    let need_clone = !target.exists();
    if need_clone {
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let mut cmd = Command::new("git");
    if need_clone {
        cmd.args(["clone", "--mirror", &repo.source]).arg(&target);
    } else {
        cmd.arg("-C").arg(&target).args(["remote", "update", "--prune"]);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return (
                "failed".into(),
                format!("无法启动 git：{e}（请确认系统已安装 Git 并加入 PATH）"),
            )
        }
    };

    // stderr 交给独立线程收集，避免管道写满阻塞
    let stderr = child.stderr.take();
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

    let outcome: (String, String) = loop {
        let done = {
            let mut slot = lock(&handle.child);
            match slot.as_mut() {
                // stop_sync 已终止并移除子进程
                None => Some(("stopped".to_string(), "已手动停止".to_string())),
                Some(c) => match c.try_wait() {
                    Ok(Some(st)) if st.success() => Some(("success".to_string(), String::new())),
                    Ok(Some(_)) => Some(("failed".to_string(), "git 命令执行失败".to_string())),
                    Ok(None) => None,
                    Err(e) => Some(("failed".to_string(), format!("等待 git 进程失败：{e}"))),
                },
            }
        };
        match done {
            Some(o) => break o,
            None => thread::sleep(Duration::from_millis(150)),
        }
    };
    lock(&state.sync_procs).remove(repo_id);

    let (final_status, final_msg) = outcome;
    let output = reader.join().unwrap_or_default();
    let output = output.trim().to_string();
    match final_status.as_str() {
        "success" => {
            let msg = if output.is_empty() {
                "同步完成".to_string()
            } else {
                tail(&output, 2000)
            };
            (final_status, msg)
        }
        "stopped" => (final_status, final_msg),
        _ => {
            let msg = if output.is_empty() {
                final_msg
            } else if final_msg.is_empty() {
                tail(&output, 2000)
            } else {
                format!("{}：{}", final_msg, tail(&output, 2000))
            };
            (final_status, msg)
        }
    }
}
