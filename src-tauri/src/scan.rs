//! 仓库扫描：扫描基地址内的一级子目录，读取 `.git/config` 派生源地址与备份目标。
//! 仓库列表即物理实况（目录在就在），不入库；数据库只保存同步状态。
//! 被 GUI 的 list_repos 命令与 MCP 的 list_repos / get_sync_status 工具共用。

use crate::state::{lock, AppState, Repo, TargetState};
use std::path::Path;

/// 保留远端名：fork 场景的上游远端。不作为备份目标——排除之，
/// 避免把用户的 fork 反向推送回原仓库
pub(crate) const RESERVED_TARGET_REMOTE: &str = "upstream";

/// 扫描结果：单个仓库的配置，全部由 `.git/config` 派生
#[derive(Clone, Debug)]
pub struct ScannedRepo {
    /// 目录名即仓库身份
    pub name: String,
    /// 源地址（origin 的 URL；无 origin 时为空，仓库视为「未配置」）
    pub source: String,
    /// 备份目标（远端名, URL），不含 origin 与 upstream
    pub targets: Vec<(String, String)>,
}

/// 解析仓库 .git/config 中的远端表（name -> url），不 spawn git 进程：
/// 仓库多时逐个调用 git 子进程在 Windows 上极慢（每次数百毫秒到数秒）。
/// 支持工作树（.git 为文件，内容 gitdir: <路径>）。
/// 返回 None 表示 config 无法读取（如 OneDrive 占位文件、权限问题）——
/// 调用方应跳过该仓库，避免把“读不到”当成“没有远端”。
pub(crate) fn parse_remote_urls(repo_dir: &Path) -> Option<Vec<(String, String)>> {
    let dotgit = repo_dir.join(".git");
    let git_dir = if dotgit.is_dir() {
        dotgit
    } else if dotgit.is_file() {
        let content = std::fs::read_to_string(&dotgit).ok()?;
        let gitdir = content
            .lines()
            .find_map(|l| l.trim().strip_prefix("gitdir:"))
            .map(str::trim)?;
        let p = std::path::PathBuf::from(gitdir);
        if p.is_absolute() {
            p
        } else {
            repo_dir.join(p)
        }
    } else {
        return Some(Vec::new());
    };

    let content = std::fs::read_to_string(git_dir.join("config")).ok()?;
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
    Some(remotes)
}

/// 扫描单个目录：非 git 仓库或 config 读不到时返回 None
fn scan_dir(path: &Path, name: &str) -> Option<ScannedRepo> {
    if !path.join(".git").exists() {
        return None;
    }
    let remotes = parse_remote_urls(path)?;
    let source = remotes
        .iter()
        .find(|(n, _)| n == "origin")
        .map(|(_, u)| u.clone())
        .unwrap_or_default();
    let targets: Vec<(String, String)> = remotes
        .into_iter()
        .filter(|(n, _)| n != "origin" && n != RESERVED_TARGET_REMOTE)
        .collect();
    Some(ScannedRepo {
        name: name.to_string(),
        source,
        targets,
    })
}

/// 扫描基地址下的全部 git 仓库（一级子目录，目录名即身份），按名称排序
pub fn scan_all(base_dir: &Path) -> Vec<ScannedRepo> {
    let mut out: Vec<ScannedRepo> = Vec::new();
    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if let Some(r) = scan_dir(&path, name) {
            out.push(r);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 扫描单个仓库（同步时按名取配置，避免全量扫描）。
/// 名称来自外部输入，需挡住路径穿越与隐藏目录。
pub fn scan_one(base_dir: &Path, name: &str) -> Option<ScannedRepo> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
    {
        return None;
    }
    scan_dir(&base_dir.join(name), name)
}

/// 扫描基地址并合并同步状态，得到仓库列表（真实列表：物理目录在就在，
/// 目录消失即从列表消失，不会被同步重新拉回）
pub fn list_repos(state: &AppState) -> Vec<Repo> {
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let scanned = scan_all(Path::new(&base_dir));
    let conn = lock(&state.conn);
    let repo_states = crate::state::load_repo_states(&conn);
    let target_states = crate::state::load_target_states(&conn);
    scanned
        .into_iter()
        .map(|s| {
            let st = repo_states.get(&s.name).cloned().unwrap_or_default();
            let targets: Vec<TargetState> = s
                .targets
                .into_iter()
                .map(|(remote, url)| {
                    let (last_status, last_message, last_synced) = target_states
                        .get(&(s.name.clone(), remote.clone()))
                        .cloned()
                        .unwrap_or_else(|| ("idle".to_string(), None, None));
                    TargetState {
                        remote,
                        url,
                        last_status,
                        last_message,
                        last_synced,
                    }
                })
                .collect();
            Repo {
                name: s.name,
                source: s.source,
                last_synced: st.last_synced,
                last_status: st.last_status,
                last_message: st.last_message,
                targets,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    /// 扫描基地址：只收录 git 仓库，origin 为源、其余远端为目标、upstream 排除
    #[test]
    fn scan_all_collects_git_repos() {
        let root = std::env::temp_dir().join(format!("grs-scan-{}", uuid::Uuid::new_v4().simple()));
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let mk = |name: &str, config: &str| {
            let d = base.join(name);
            std::fs::create_dir_all(d.join(".git")).unwrap();
            std::fs::write(d.join(".git/config"), config).unwrap();
        };
        mk(
            "alpha",
            "[remote \"origin\"]\n\turl = https://github.com/u/alpha.git\n\
             [remote \"backup\"]\n\turl = https://gitlab.com/u/alpha.git\n",
        );
        mk(
            "gamma",
            "[remote \"origin\"]\n\turl = https://github.com/u/gamma.git\n\
             [remote \"upstream\"]\n\turl = https://github.com/orig/gamma.git\n",
        );
        // 非 git 目录与隐藏目录应被跳过
        std::fs::create_dir_all(base.join("notrepo")).unwrap();
        std::fs::create_dir_all(base.join(".hid")).unwrap();

        let repos = scan_all(&base);
        assert_eq!(repos.len(), 2, "只收录 git 仓库: {repos:?}");
        assert_eq!(repos[0].name, "alpha");
        assert_eq!(repos[0].source, "https://github.com/u/alpha.git");
        assert_eq!(
            repos[0].targets,
            vec![("backup".to_string(), "https://gitlab.com/u/alpha.git".to_string())]
        );
        assert_eq!(repos[1].name, "gamma");
        assert!(repos[1].targets.is_empty(), "upstream 不作为备份目标");

        std::fs::remove_dir_all(&root).ok();
    }

    /// 列表即物理实况：目录删除后从列表消失，状态保留在库里；
    /// 目录回来后重新出现并带回上次同步状态
    #[test]
    fn list_repos_follows_physical_state() {
        let root =
            std::env::temp_dir().join(format!("grs-scanlist-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join("base");
        let demo = base.join("demo");
        std::fs::create_dir_all(demo.join(".git")).unwrap();
        std::fs::write(
            demo.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/u/demo.git\n\
             [remote \"backup\"]\n\turl = https://gitlab.com/u/demo.git\n",
        )
        .unwrap();

        let state = AppState::open(root.join("app.db")).unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());
        // 预置同步状态：仓库级 + 目标级
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repo_state (name, last_synced, last_status) VALUES ('demo', 12345, 'success')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO target_state (repo_name, remote, last_synced, last_status)
                 VALUES ('demo', 'backup', 23456, 'success')",
                [],
            )
            .unwrap();
        }

        let list = list_repos(&state);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "demo");
        assert_eq!(list[0].source, "https://github.com/u/demo.git");
        assert_eq!(list[0].last_status, "success");
        assert_eq!(list[0].last_synced, Some(12345));
        assert_eq!(list[0].targets.len(), 1);
        assert_eq!(list[0].targets[0].last_status, "success");
        assert_eq!(list[0].targets[0].last_synced, Some(23456));

        // 物理目录被删除：仓库从列表消失（不会再被同步拉回），状态仍留在库里
        std::fs::remove_dir_all(&demo).unwrap();
        assert!(list_repos(&state).is_empty(), "物理目录删除后应从列表消失");

        // 目录回来：重新出现并带回上次同步状态
        std::fs::create_dir_all(demo.join(".git")).unwrap();
        std::fs::write(
            demo.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/u/demo.git\n\
             [remote \"backup\"]\n\turl = https://gitlab.com/u/demo.git\n",
        )
        .unwrap();
        let list = list_repos(&state);
        assert_eq!(list.len(), 1, "目录回来后应重新出现");
        assert_eq!(list[0].last_status, "success", "同步状态应被保留");

        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// scan_one：只扫指定仓库，并挡住路径穿越与隐藏目录
    #[test]
    fn scan_one_guards_path_traversal() {
        let root =
            std::env::temp_dir().join(format!("grs-scanone-{}", uuid::Uuid::new_v4().simple()));
        let base = root.join("base");
        std::fs::create_dir_all(base.join("demo/.git")).unwrap();
        std::fs::write(
            base.join("demo/.git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/u/demo.git\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("secret/.git")).unwrap();

        assert_eq!(
            scan_one(&base, "demo").map(|r| r.source),
            Some("https://github.com/u/demo.git".to_string())
        );
        assert!(scan_one(&base, "..").is_none(), "拒绝上级目录");
        assert!(scan_one(&base, "../secret").is_none(), "拒绝路径穿越");
        assert!(scan_one(&base, ".hidden").is_none(), "拒绝隐藏目录");
        assert!(scan_one(&base, "").is_none(), "拒绝空名");

        std::fs::remove_dir_all(&root).ok();
    }

    /// 解析 .git/config 的远端表：多远端按出现顺序返回
    #[test]
    fn parse_remote_urls_reads_git_config() {
        let root =
            std::env::temp_dir().join(format!("grs-parse-{}", uuid::Uuid::new_v4().simple()));
        let repo = root.join("r");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(
            repo.join(".git/config"),
            "[core]\n\tbare = false\n\
             [remote \"origin\"]\n\turl = https://github.com/u/r.git\n\
             [remote \"backup\"]\n\turl = git@gitlab.com:u/r.git\n",
        )
        .unwrap();
        let remotes = parse_remote_urls(&repo).unwrap();
        assert_eq!(
            remotes,
            vec![
                ("origin".to_string(), "https://github.com/u/r.git".to_string()),
                ("backup".to_string(), "git@gitlab.com:u/r.git".to_string()),
            ]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// 工作树（.git 为文件，指向 gitdir）同样能解析远端
    #[test]
    fn parse_remote_urls_supports_gitfile_worktree() {
        let root =
            std::env::temp_dir().join(format!("grs-wt-{}", uuid::Uuid::new_v4().simple()));
        let wt = root.join("wt");
        let real = root.join("real/.git");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            real.join("config"),
            "[remote \"origin\"]\n\turl = https://github.com/u/wt.git\n",
        )
        .unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        let remotes = parse_remote_urls(&wt).unwrap();
        assert_eq!(
            remotes,
            vec![("origin".to_string(), "https://github.com/u/wt.git".to_string())]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// 无 .git 的目录返回空远端表
    #[test]
    fn parse_remote_urls_empty_without_git() {
        let root =
            std::env::temp_dir().join(format!("grs-nogit-{}", uuid::Uuid::new_v4().simple()));
        let d = root.join("d");
        std::fs::create_dir_all(&d).unwrap();
        assert!(parse_remote_urls(&d).unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// config 读不到时返回 None（调用方据此跳过，不误判为「没有远端」）
    #[test]
    fn parse_remote_urls_none_when_config_unreadable() {
        let root = std::env::temp_dir().join(format!("grs-nocfg-{}", uuid::Uuid::new_v4().simple()));
        let d = root.join("d/.git");
        std::fs::create_dir_all(&d).unwrap();
        assert!(parse_remote_urls(&root.join("d")).is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
