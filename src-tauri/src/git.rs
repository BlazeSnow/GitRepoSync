//! 同步流水线：git 子进程执行与「拉取 → 更新 → 推送」步骤编排。
//! 被同步队列（sync.rs）调用；自身不触碰 repos 表状态，只更新 sync_targets 行。

use crate::lang::{tr, tr_a, Lang};
use crate::state::{lock, now_ms, AppState, Repo, SyncHandle};
use rusqlite::params;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// 单个 git 网络命令的超时（秒）；LFS fetch --all 首次拉取大仓库较慢，单独放宽
const GIT_TIMEOUT_SECS: u64 = 1800;
const LFS_TIMEOUT_SECS: u64 = 3600;

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

/// GUI 进程（macOS 从 Finder/Dock 启动）继承的 PATH 极简（/usr/bin:/bin:/usr/sbin:/sbin），
/// 用户级安装的 git-lfs（Homebrew `/opt/homebrew/bin` 等）不在其中，`git lfs` 子命令
/// 因找不到 git-lfs 可执行文件而误报未安装。为 git 子进程补充常见安装目录：
/// 目录存在且未收录才追加，原 PATH 条目按原顺序保留在前（系统 git 仍优先）。
fn augmented_path() -> String {
    const EXTRA: &[&str] = &[
        "/opt/homebrew/bin",              // Homebrew（Apple Silicon）
        "/opt/homebrew/sbin",
        "/usr/local/bin",                 // Homebrew（Intel）与常规用户安装
        "/usr/local/sbin",
        "/opt/local/bin",                 // MacPorts
        "/opt/local/sbin",
        "/home/linuxbrew/.linuxbrew/bin", // Linuxbrew
        "/home/linuxbrew/.linuxbrew/sbin",
    ];
    let mut parts: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    for dir in EXTRA {
        let p = Path::new(dir);
        if p.is_dir() && !parts.iter().any(|e| e.as_path() == p) {
            parts.push(p.to_path_buf());
        }
    }
    std::env::join_paths(&parts)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// git 子进程使用的 PATH（进程存活期间环境不变，进程内缓存一次）
fn child_path() -> &'static str {
    static CHILD_PATH: OnceLock<String> = OnceLock::new();
    CHILD_PATH.get_or_init(augmented_path)
}

/// git 子进程统一环境：禁用交互式凭据输入 + 补充 PATH（图形界面启动时 PATH 极简）
fn apply_git_env(cmd: &mut Command) {
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    let p = child_path();
    if !p.is_empty() {
        cmd.env("PATH", p);
    }
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
pub(crate) fn perform_git_sync(
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
        apply_git_env(&mut cmd);
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
pub(crate) fn reset_running_targets(state: &AppState, repo_id: &str) {
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
    apply_git_env(&mut cmd);
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

    /// 错误消息里的 git 子命令名：跳过注入的配置前缀（两组 -c k v）
    #[test]
    fn git_display_cmd_skips_config_prefix() {
        let args = git_args(&["fetch", "origin"]);
        assert_eq!(git_display_cmd(&args), "fetch");
        assert_eq!(git_display_cmd(&["push".to_string()]), "push");
        assert_eq!(git_display_cmd(&[]), "git");
    }

    /// PATH 增强（macOS 图形界面启动找不到 Homebrew git-lfs 的修复）：
    /// 原 PATH 条目按原顺序原样保留在前（用户 PATH 可能本就含重复，不去重），
    /// 补充目录存在且未收录时才追加，追加部分自身无重复
    #[test]
    fn augmented_path_keeps_original_and_appends_existing_extras() {
        let orig = std::env::var("PATH").unwrap_or_default();
        let orig_parts: Vec<PathBuf> = std::env::split_paths(&orig).collect();
        let aug = augmented_path();
        let aug_parts: Vec<PathBuf> = std::env::split_paths(&aug).collect();
        assert_eq!(
            &aug_parts[..orig_parts.len()],
            &orig_parts[..],
            "原 PATH 条目应按原顺序保留在前"
        );
        let extras = &aug_parts[orig_parts.len()..];
        let mut seen = std::collections::HashSet::new();
        for p in extras {
            assert!(seen.insert(p), "追加部分出现重复条目: {p:?}");
        }
        // 追加的都是「目录存在且原 PATH 未收录」的常见安装目录
        const EXTRA: &[&str] = &[
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
            "/opt/local/bin",
            "/opt/local/sbin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/home/linuxbrew/.linuxbrew/sbin",
        ];
        for p in extras {
            assert!(
                EXTRA.iter().any(|d| Path::new(d) == p.as_path()),
                "追加了预期之外的条目: {p:?}"
            );
        }
    }

    /// 全本地端到端：真实 git 子进程走完「拉取 → 更新 → 推送」流水线。
    /// 源仓库与目标 bare 仓库均用本地路径，不依赖网络与凭据；
    /// 第二次同步走 fetch 更新路径（本地中转已存在），验证增量推送到目标。
    #[test]
    fn perform_git_sync_full_pipeline_with_local_git() {
        use crate::state::AppState;

        let root = std::env::temp_dir().join(format!("grs-sync-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join("base");
        let source = root.join("src");
        let target = root.join("target.git");
        let git = |args: &[&str], cwd: &Path| {
            let st = std::process::Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(st.success(), "git {:?} 执行失败", args);
        };
        // 源仓库：main 分支一次提交；目标：空 bare 仓库（路径作为参数由 git 创建）
        std::fs::create_dir_all(&source).unwrap();
        git(&["init", "-q", "-b", "main"], &source);
        git(&["config", "user.name", "t"], &source);
        git(&["config", "user.email", "t@t"], &source);
        std::fs::write(source.join("a.txt"), "v1").unwrap();
        git(&["add", "."], &source);
        git(&["commit", "-q", "-m", "v1"], &source);
        git(&["init", "-q", "--bare", target.to_str().unwrap()], &root);

        let state = AppState::open(root.join("app.db")).unwrap();
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
        let repo = Repo {
            id: "r1".to_string(),
            name: "demo".to_string(),
            source: source.to_string_lossy().to_string(),
            last_synced: None,
            last_status: "idle".to_string(),
            last_message: None,
            targets: Vec::new(),
        };
        let targets = vec![("backup".to_string(), target.to_string_lossy().to_string())];
        let target_sha = || {
            let out = std::process::Command::new("git")
                .args(["-C", target.to_str().unwrap(), "rev-parse", "refs/heads/main"])
                .env("GIT_TERMINAL_PROMPT", "0")
                .output()
                .unwrap();
            assert!(out.status.success(), "目标 bare 仓库应已有 main 分支");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        // 第一次同步：克隆中转 → 推送分支与标签
        let (status, message) =
            perform_git_sync(&state, "r1", &repo, &targets, &base, crate::lang::Lang::Zh);
        assert_eq!(status, "success", "message: {message}");
        assert_eq!(target_sha(), rev_parse(&source, "HEAD"), "目标与源提交一致");
        let sha_after_first = target_sha();

        // 第二次同步：走 fetch 更新路径，源新增提交应推进目标
        std::fs::write(source.join("a.txt"), "v2").unwrap();
        git(&["add", "."], &source);
        git(&["commit", "-q", "-m", "v2"], &source);
        let (status2, message2) =
            perform_git_sync(&state, "r1", &repo, &targets, &base, crate::lang::Lang::Zh);
        assert_eq!(status2, "success", "message: {message2}");
        assert_ne!(target_sha(), sha_after_first, "第二次同步应推进目标分支");
        assert_eq!(target_sha(), rev_parse(&source, "HEAD"), "目标与源最新提交一致");

        // 目标行状态与同步时间已记录
        {
            let conn = lock(&state.conn);
            let (st, ts): (String, Option<i64>) = conn
                .query_row(
                    "SELECT last_status, last_synced FROM sync_targets WHERE repo_id = 'r1' AND remote = 'backup'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(st, "success");
            assert!(ts.is_some(), "目标同步时间已更新");
        }
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 源地址不可达：克隆阶段失败，整条同步判定失败且消息非空
    #[test]
    fn perform_git_sync_fails_when_source_unreachable() {
        use crate::state::AppState;

        let root = std::env::temp_dir().join(format!("grs-sync-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::open(root.join("app.db")).unwrap();
        let repo = Repo {
            id: "r1".to_string(),
            name: "demo".to_string(),
            source: root.join("no-such-repo").to_string_lossy().to_string(),
            last_synced: None,
            last_status: "idle".to_string(),
            last_message: None,
            targets: Vec::new(),
        };
        let targets = vec![(
            "backup".to_string(),
            root.join("target.git").to_string_lossy().to_string(),
        )];
        let (status, message) =
            perform_git_sync(&state, "r1", &repo, &targets, &root, crate::lang::Lang::Zh);
        assert_eq!(status, "failed");
        assert!(!message.is_empty(), "失败应带可读消息");
        // 失败后不应留有 git 子进程句柄
        assert!(lock(&state.sync_procs).is_empty());
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    fn rev_parse(path: &Path, spec: &str) -> String {
        let out = std::process::Command::new("git")
            .args(["-C", path.to_str().unwrap(), "rev-parse", spec])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(out.status.success(), "git rev-parse {spec} 失败");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}
