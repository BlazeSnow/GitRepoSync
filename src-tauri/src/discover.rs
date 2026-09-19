//! 自动发现：扫描基地址内的一级子目录，把 git 仓库登记进列表。
//! 被 GUI 的 discover_repos 命令与 MCP 的 discover_repos 工具共用。

use crate::lang::{tr_a, Lang};
use crate::state::{lock, AppState};
use rusqlite::params;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// 解析仓库 .git/config 中的远端表（name -> url），不 spawn git 进程：
/// 仓库多时逐个调用 git 子进程在 Windows 上极慢（每次数百毫秒到数秒）。
/// 支持工作树（.git 为文件，内容 gitdir: <路径>）。
/// 返回 None 表示 config 无法读取（如 OneDrive 占位文件、权限问题）——
/// 调用方应跳过该仓库，避免把“读不到”当成“没有远端”而误删已有目标。
fn parse_remote_urls(repo_dir: &Path) -> Option<Vec<(String, String)>> {
    let dotgit = repo_dir.join(".git");
    let git_dir = if dotgit.is_dir() {
        dotgit
    } else if dotgit.is_file() {
        let content = std::fs::read_to_string(&dotgit).ok()?;
        let gitdir = content
            .lines()
            .find_map(|l| l.trim().strip_prefix("gitdir:"))
            .map(str::trim)?;
        let p = PathBuf::from(gitdir);
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

/// 扫描基地址下的一级子目录，自动登记未入库的 git 仓库：
/// origin 远端作为源地址、其余全部远端作为备份目标；
/// 已登记的仓库仅补填空地址与同步远端集合，不覆盖用户手动修改的值；隐藏（已删除）的仓库跳过。
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
            let Some(remotes) = parse_remote_urls(&path) else {
                continue; // config 读不到：跳过，不动数据库里的已有目标
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[test]
    fn discover_registers_and_patches() {
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
        assert_eq!(
            parse_remote_urls(&repo).unwrap(),
            vec![
                ("origin".to_string(), "https://github.com/u/r.git".to_string()),
                ("backup".to_string(), "git@gitlab.com:u/r.git".to_string()),
            ]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// 工作树形态：.git 为文件，gitdir: 指向真实 git 目录
    #[test]
    fn parse_remote_urls_supports_gitfile_worktree() {
        let root =
            std::env::temp_dir().join(format!("grs-parse-{}", uuid::Uuid::new_v4().simple()));
        let repo = root.join("wt");
        let real = root.join("real.git");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(repo.join(".git"), format!("gitdir: {}", real.to_string_lossy())).unwrap();
        std::fs::write(real.join("config"), "[remote \"origin\"]\n\turl = https://x/r.git\n")
            .unwrap();
        assert_eq!(
            parse_remote_urls(&repo).unwrap(),
            vec![("origin".to_string(), "https://x/r.git".to_string())]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// .git 存在但 config 读不到：返回 None（调用方跳过，不当作“没有远端”误删目标）
    #[test]
    fn parse_remote_urls_none_when_config_unreadable() {
        let root =
            std::env::temp_dir().join(format!("grs-parse-{}", uuid::Uuid::new_v4().simple()));
        let repo = root.join("broken");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(parse_remote_urls(&repo), None);
        std::fs::remove_dir_all(&root).ok();
    }

    /// 非 git 目录返回空列表（而非 None）
    #[test]
    fn parse_remote_urls_empty_without_git() {
        let root =
            std::env::temp_dir().join(format!("grs-parse-{}", uuid::Uuid::new_v4().simple()));
        let repo = root.join("plain");
        std::fs::create_dir_all(&repo).unwrap();
        assert_eq!(parse_remote_urls(&repo), Some(Vec::new()));
        std::fs::remove_dir_all(&root).ok();
    }
}
