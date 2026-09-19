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
        last_synced: row.get(4)?,
        last_status: row.get(5)?,
        last_message: row.get(6)?,
        targets: Vec::new(),
    })
}

const REPO_COLS: &str = "id, name, source, target, last_synced, last_status, last_message";

#[tauri::command]
pub fn list_repos(state: State<'_, Arc<AppState>>, token: String) -> Result<Vec<Repo>, String> {
    require_session(&state, &token)?;
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
    Ok(repos)
}

#[tauri::command]
pub fn save_repo(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: Option<String>,
    name: String,
    source: String,
    targets: Vec<TargetInput>,
) -> Result<Repo, String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    let name = name.trim().to_string();
    let source = source.trim().to_string();
    if name.is_empty() || source.is_empty() {
        return Err(tr(lang, "repo-fields-empty"));
    }
    // 目标清洗：去空行、remote/url 去空白
    let targets: Vec<TargetInput> = targets
        .into_iter()
        .map(|t| TargetInput {
            remote: t.remote.trim().to_string(),
            url: t.url.trim().to_string(),
        })
        .filter(|t| !t.remote.is_empty() && !t.url.is_empty())
        .collect();
    let repo = {
        let conn = lock(&state.conn);
        if let Some(rid) = &id {
            let updated = conn
                .execute(
                    "UPDATE repos SET name = ?1, source = ?2 WHERE id = ?3",
                    params![name, source, rid],
                )
                .map_err(|e| e.to_string())?;
            if updated == 0 {
                return Err(tr(lang, "repo-not-found"));
            }
            conn.execute("DELETE FROM sync_targets WHERE repo_id = ?1", params![rid])
                .map_err(|e| e.to_string())?;
            for t in &targets {
                conn.execute(
                    "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)",
                    params![rid, t.remote, t.url],
                )
                .map_err(|e| e.to_string())?;
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
                last_synced: None,
                last_status: "idle".into(),
                last_message: None,
                targets: Vec::new(),
            };
            conn.execute(
                "INSERT INTO repos (id, name, source, last_synced, last_status, last_message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    repo.id,
                    repo.name,
                    repo.source,
                    repo.last_synced,
                    repo.last_status,
                    repo.last_message
                ],
            )
            .map_err(|e| e.to_string())?;
            for t in &targets {
                conn.execute(
                    "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)",
                    params![repo.id, t.remote, t.url],
                )
                .map_err(|e| e.to_string())?;
            }
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
        // 标记隐藏而非物理删除：目录仍在基地址内时避免被自动发现反复登记
        conn.execute("UPDATE repos SET hidden = 1 WHERE id = ?1", params![id])
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

/// save_repo 的单个备份目标输入
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInput {
    pub remote: String,
    pub url: String,
}

/// 解析仓库 .git/config 中的远端表（name -> url），不 spawn git 进程：
/// 仓库多时逐个调用 git 子进程在 Windows 上极慢（每次数百毫秒到数秒）。
/// 支持工作树（.git 为文件，内容 gitdir: <路径>）。
fn parse_remote_urls(repo_dir: &Path) -> Vec<(String, String)> {
    let dotgit = repo_dir.join(".git");
    let git_dir = if dotgit.is_dir() {
        dotgit
    } else if dotgit.is_file() {
        let content = match std::fs::read_to_string(&dotgit) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let Some(gitdir) = content
            .lines()
            .find_map(|l| l.trim().strip_prefix("gitdir:"))
            .map(str::trim)
        else {
            return Vec::new();
        };
        let p = PathBuf::from(gitdir);
        if p.is_absolute() {
            p
        } else {
            repo_dir.join(p)
        }
    } else {
        return Vec::new();
    };

    let Ok(content) = std::fs::read_to_string(git_dir.join("config")) else {
        return Vec::new();
    };
    let mut remotes: Vec<(String, String)> = Vec::new();
    let mut current: Option<String> = None;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            current = None;
            let inner = line.trim_start_matches('[').trim_end_matches(']');
            if let Some(rest) = inner.strip_prefix("remote") {
                let name = rest.trim().trim_matches('"');
                if !name.is_empty() {
                    current = Some(name.to_string());
                }
            }
        } else if let Some(name) = &current {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == "url" && !remotes.iter().any(|(n, _)| n == name) {
                    remotes.push((name.clone(), v.trim().to_string()));
                }
            }
        }
    }
    remotes
}

/// 扫描基地址下的一级子目录，自动登记未入库的 git 仓库：
/// origin 远端作为源地址、backup 远端作为目标地址，缺失以空串存储（界面显示“未配置”）；
/// 已登记的仓库仅补填空地址，不覆盖用户手动修改的值；隐藏（已删除）的仓库跳过。
pub fn discover(state: &AppState, operator: &str, lang: Lang) -> Result<(), String> {
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let base = PathBuf::from(&base_dir);

    let mut found: Vec<(String, Option<String>, Vec<(String, String)>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&base) {
        for e in entries.flatten() {
            let path = e.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') || !path.join(".git").exists() {
                continue;
            }
            let remotes = parse_remote_urls(&path);
            let origin = remotes
                .iter()
                .find(|(n, _)| n == "origin")
                .map(|(_, u)| u.clone());
            let targets: Vec<(String, String)> = remotes
                .into_iter()
                .filter(|(n, _)| n != "origin")
                .collect();
            found.push((name.to_string(), origin, targets));
        }
    }
    found.sort();

    let mut to_register: Vec<(String, String, Vec<(String, String)>)> = Vec::new();
    {
        let conn = lock(&state.conn);
        for (name, origin, targets) in found {
            let hidden: i64 = conn
                .query_row(
                    "SELECT hidden FROM repos WHERE name = ?1",
                    params![name],
                    |r| r.get(0),
                )
                .unwrap_or(2); // 2 = 无记录
            match hidden {
                1 => continue, // 用户已删除，跳过
                0 => {
                    let repo_id: String = conn
                        .query_row(
                            "SELECT id FROM repos WHERE name = ?1",
                            params![name],
                            |r| r.get(0),
                        )
                        .unwrap_or_default();
                    if repo_id.is_empty() {
                        continue;
                    }
                    // 补空 source，不覆盖手动修改
                    if let Some(o) = &origin {
                        let _ = conn.execute(
                            "UPDATE repos SET source = ?1 WHERE id = ?2 AND source = ''",
                            params![o, repo_id],
                        );
                    }
                    // 同步目标远端集合：删除已不存在的远端，补/更新现有远端 URL
                    let existing: Vec<String> = {
                        let mut stmt = conn
                            .prepare("SELECT remote FROM sync_targets WHERE repo_id = ?1")
                            .map_err(|e| e.to_string())?;
                        let rows = stmt
                            .query_map(params![repo_id], |r| r.get::<_, String>(0))
                            .map_err(|e| e.to_string())?
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|e| e.to_string())?;
                        rows
                    };
                    for r in &existing {
                        if !targets.iter().any(|(name, _)| name == r) {
                            let _ = conn.execute(
                                "DELETE FROM sync_targets WHERE repo_id = ?1 AND remote = ?2",
                                params![repo_id, r],
                            );
                        }
                    }
                    for (remote, url) in &targets {
                        let _ = conn.execute(
                            "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)
                             ON CONFLICT(repo_id, remote) DO UPDATE SET url = excluded.url",
                            params![repo_id, remote, url],
                        );
                    }
                }
                _ => {
                    to_register.push((name, origin.unwrap_or_default(), targets));
                }
            }
        }
        for (name, source, targets) in &to_register {
            let repo_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO repos (id, name, source, target, last_synced, last_status, last_message, hidden)
                 VALUES (?1, ?2, ?3, '', NULL, 'idle', NULL, 0)",
                params![repo_id, name, source],
            )
            .map_err(|e| e.to_string())?;
            for (remote, url) in targets {
                conn.execute(
                    "INSERT INTO sync_targets (repo_id, remote, url) VALUES (?1, ?2, ?3)",
                    params![repo_id, remote, url],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    for (name, _, _) in &to_register {
        state.add_log(&tr_a(lang, "log-repo-discovered", &[("name", name)]), operator);
    }
    Ok(())
}



/// 主界面加载入口：先自动发现基地址内仓库，再返回最新列表
#[tauri::command]
pub fn discover_repos(state: State<'_, Arc<AppState>>, token: String) -> Result<Vec<Repo>, String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    discover(&state, &username, lang)?;
    let conn = lock(&state.conn);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {REPO_COLS} FROM repos WHERE hidden = 0 ORDER BY name"
        ))
        .map_err(|e| e.to_string())?;
    let repos = stmt
        .query_map([], repo_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(repos)
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
    reset_running_targets(state, repo_id);
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

struct GitStep {
    args: Vec<String>,
    cwd: Option<PathBuf>,
    ok_msg: String,
    timeout: Duration,
    /// 失败是否判定整个同步失败；LFS / submodule 失败仅警告（引用备份仍然有效）
    fatal: bool,
    warn_key: &'static str,
}

/// git_args 注入的配置前缀参数个数（两组 -c k v）
const GIT_CONFIG_PREFIX: usize = 4;

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
    targets: &[(String, String)],
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

    // 1 对多推送：对每个备份目标依次推送，状态按目标独立记录；
    // 单个目标失败不阻断其余目标（最后汇总整体状态为 failed）
    let mut any_fail = false;
    for (remote, url) in targets {
        if lock(&state.stop_requested).contains(repo_id) {
            reset_running_targets(state, repo_id);
            return ("stopped".into(), tr(lang, "manually-stopped"));
        }
        let mut push_cmd: Vec<String> =
            vec!["push".into(), "--prune".into(), url.clone()];
        push_cmd.extend(push_refs.clone());
        let target_status;
        match run_git(
            state,
            repo_id,
            &git_args(&push_cmd.iter().map(String::as_str).collect::<Vec<_>>()),
            Some(local.as_path()),
            git_timeout,
            lang,
        ) {
            Err(e) => {
                if lock(&state.stop_requested).contains(repo_id) {
                    reset_running_targets(state, repo_id);
                    return ("stopped".into(), tr(lang, "manually-stopped"));
                }
                any_fail = true;
                target_status = ("failed".to_string(), Some(e));
            }
            Ok(_) => {
                target_status = ("success".to_string(), None);
            }
        }
        let conn = lock(&state.conn);
        let _ = conn.execute(
            "UPDATE sync_targets SET last_status = ?1, last_message = ?2, last_synced = ?3
             WHERE repo_id = ?4 AND remote = ?5",
            params![
                target_status.0,
                target_status.1,
                if target_status.0 == "success" { Some(now_ms()) } else { None },
                repo_id,
                remote
            ],
        );
    }
    done.push(tr(lang, "step-push"));

    let mut parts = done;
    parts.extend(warnings);
    let status = if any_fail { "failed" } else { "success" };
    (status.into(), parts.join(lang.sep()))
}

/// 停止同步后把仍处于 running 的目标行复位为 idle
fn reset_running_targets(state: &AppState, repo_id: &str) {
    let conn = lock(&state.conn);
    let _ = conn.execute(
        "UPDATE sync_targets SET last_status = 'idle' WHERE repo_id = ?1 AND last_status = 'running'",
        params![repo_id],
    );
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
            eprintln!("git {:?} 执行失败：{text}", git_display_cmd(args));
            Err(if text.is_empty() {
                format!("git {:?} 执行失败", git_display_cmd(args))
            } else {
                text
            })
        }
    }
}

/// 错误消息中的 git 子命令名（跳过注入的配置前缀）
fn git_display_cmd(args: &[String]) -> String {
    args.get(GIT_CONFIG_PREFIX)
        .or_else(|| args.first())
        .cloned()
        .unwrap_or_else(|| "git".into())
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
    fn discover_registers_and_patches() {
        use crate::state::AppState;

        let root = std::env::temp_dir().join(format!("grs-test-{}", uuid::Uuid::new_v4().simple()));
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let st = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(st.success(), "git {:?} failed", args);
        };

        // alpha：origin + backup 都配置
        let alpha = base.join("alpha");
        std::fs::create_dir_all(&alpha).unwrap();
        git(&["init", "-q"], &alpha);
        git(&["remote", "add", "origin", "https://github.com/u/alpha.git"], &alpha);
        git(&["remote", "add", "backup", "https://gitlab.com/u/alpha.git"], &alpha);
        // beta：仅 origin
        let beta = base.join("beta");
        std::fs::create_dir_all(&beta).unwrap();
        git(&["init", "-q"], &beta);
        git(&["remote", "add", "origin", "https://github.com/u/beta.git"], &beta);
        // 非 git 目录与隐藏目录应被跳过
        std::fs::create_dir_all(base.join("notrepo")).unwrap();
        std::fs::create_dir_all(base.join(".hid")).unwrap();

        let state = AppState::open(root.join("app.db")).unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());
        discover(&state, "test", crate::lang::Lang::Zh).unwrap();

        let list = || -> Vec<(String, String, Vec<(String, String)>)> {
            let conn = lock(&state.conn);
            let mut stmt = conn
                .prepare("SELECT id, name, source FROM repos WHERE hidden = 0 ORDER BY name")
                .unwrap();
            let mut rows: Vec<(String, String, String)> = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let mut out = Vec::new();
            for (id, name, source) in rows.drain(..) {
                let mut ts = conn
                    .prepare("SELECT remote, url FROM sync_targets WHERE repo_id = ?1 ORDER BY remote")
                    .unwrap();
                let targets: Vec<(String, String)> = ts
                    .query_map([&id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                out.push((name, source, targets));
            }
            out
        };
        let rows = list();
        assert_eq!(rows.len(), 2, "only git repos registered: {rows:?}");
        assert_eq!(rows[0].0, "alpha");
        assert_eq!(rows[0].1, "https://github.com/u/alpha.git");
        assert_eq!(
            rows[0].2,
            vec![("backup".to_string(), "https://gitlab.com/u/alpha.git".to_string())]
        );
        assert_eq!(rows[1].0, "beta");
        assert!(rows[1].2.is_empty(), "beta has no backup remote yet");

        // 幂等：再次发现不产生重复
        discover(&state, "test", crate::lang::Lang::Zh).unwrap();
        assert_eq!(list().len(), 2);

        // beta 后来加了 backup 远端，再次发现应自动补为目标
        git(&["remote", "add", "backup", "https://gitlab.com/u/beta.git"], &beta);
        discover(&state, "test", crate::lang::Lang::Zh).unwrap();
        let rows = list();
        assert_eq!(rows[1].2, vec![("backup".to_string(), "https://gitlab.com/u/beta.git".to_string())]);

        // 移除远端后目标同步删除
        git(&["remote", "remove", "backup"], &beta);
        discover(&state, "test", crate::lang::Lang::Zh).unwrap();
        let rows = list();
        assert!(rows[1].2.is_empty());

        // 隐藏的仓库不再被登记：隐藏 alpha 后其目录仍在基地址内
        {
            let conn = lock(&state.conn);
            conn.execute("UPDATE repos SET hidden = 1 WHERE name = 'alpha'", []).unwrap();
        }
        discover(&state, "test", crate::lang::Lang::Zh).unwrap();
        let rows = list();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "beta");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn default_base_dir_is_absolute() {
        let d = crate::state::default_base_dir();
        assert!(d.ends_with("repo"));
        assert!(!d.starts_with('~'));
    }
}
