//! 仓库的 Tauri 命令：列表（含自动发现）、隐藏（软删除）、打开目录。
//! 仓库配置只读：源与目标由 .git/config 派生（origin 为源、其余非
//! upstream 远端为目标），由用户自行用 git remote 管理，软件不代管。
//! 同步任务见 sync.rs；发现逻辑见 discover.rs。

use crate::auth::require_session;
use crate::discover::discover;
use crate::lang::{gui_lang, tr, tr_a, Lang};
use crate::state::{lock, repo_from_row, AppState, Repo, REPO_COLS};
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

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
pub fn delete_repo(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: String,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    let lang = gui_lang();
    if state.job_active(&id) {
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
    let mut repos = stmt
        .query_map([], repo_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    crate::state::attach_targets(&conn, &mut repos);
    Ok(repos)
}

/// 解析仓库的本地中转目录（{基地址}/{仓库名}）并检查存在性；
/// 独立成函数便于单测（不实际拉起文件管理器）
fn resolve_repo_dir(state: &AppState, id: &str, lang: Lang) -> Result<PathBuf, String> {
    let name: String = {
        let conn = lock(&state.conn);
        conn.query_row("SELECT name FROM repos WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .map_err(|_| tr(lang, "repo-not-found"))?
    };
    let base_dir = state
        .get_setting("base_dir")
        .unwrap_or_else(crate::state::default_base_dir);
    let dir = Path::new(&base_dir).join(name);
    if !dir.is_dir() {
        return Err(tr_a(
            lang,
            "repo-dir-not-exist",
            &[("dir", &dir.to_string_lossy())],
        ));
    }
    Ok(dir)
}

/// 在系统文件管理器中打开仓库的本地中转目录（只读导航，不写操作日志）
#[tauri::command]
pub fn open_repo_dir(
    state: State<'_, Arc<AppState>>,
    token: String,
    id: String,
) -> Result<(), String> {
    require_session(&state, &token)?;
    let dir = resolve_repo_dir(state.inner(), &id, gui_lang())?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testutil::{insert_session, open_mock_app};

    /// default_base_dir 是完整路径（state.rs 的函数，测试跟随函数所在模块语义）
    #[test]
    fn default_base_dir_is_absolute() {
        let d = crate::state::default_base_dir();
        assert!(d.ends_with("repo"));
        assert!(!d.starts_with('~'));
    }

    /// open_repo_dir 的目录解析：存在返回路径、缺失与未知 id 报错
    /// （只测解析，不实际拉起文件管理器）
    #[test]
    fn resolve_repo_dir_checks_existence() {
        let (app, state, root) = open_mock_app("opendir");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("demo")).unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'demo', 'https://src/demo.git')",
                [],
            )
            .unwrap();
        }
        assert_eq!(
            resolve_repo_dir(&state, "r1", crate::lang::Lang::Zh).unwrap(),
            base.join("demo")
        );
        // 目录缺失（尚未克隆）报错
        std::fs::remove_dir_all(base.join("demo")).unwrap();
        assert!(resolve_repo_dir(&state, "r1", crate::lang::Lang::Zh).is_err());
        // 未知 id 报“仓库不存在”
        assert!(resolve_repo_dir(&state, "nope", crate::lang::Lang::Zh).is_err());

        drop(app);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// delete_repo：有排队/运行中同步任务时拒绝；软删除后列表不可见且
    /// 重复删除幂等；无效令牌拒绝
    #[test]
    fn delete_repo_hides_and_guards() {
        use tauri::Manager;

        let (app, state, root) = open_mock_app("repos");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source) VALUES ('r1', 'demo', 'https://src/demo.git')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_targets (repo_id, remote, url) VALUES ('r1', 'backup', 'https://bak/demo.git')",
                [],
            )
            .unwrap();
        }
        let list = || -> Vec<Repo> {
            list_repos(st.clone(), "tok".into()).unwrap()
        };
        assert_eq!(list().len(), 1);

        // 有排队/运行中同步任务的仓库拒绝删除（跨进程任务表互斥）
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                 VALUES ('r1', 'mcp', 'zh', 'queued', 0)",
                [],
            )
            .unwrap();
        }
        assert!(delete_repo(st.clone(), "tok".into(), "r1".into()).is_err());
        {
            let conn = lock(&state.conn);
            conn.execute("DELETE FROM sync_jobs", []).unwrap();
        }

        // 软删除：列表不可见；重复删除幂等成功（行仍存在只是隐藏）
        delete_repo(st.clone(), "tok".into(), "r1".into()).unwrap();
        assert!(list().is_empty());
        delete_repo(st.clone(), "tok".into(), "r1".into()).unwrap();

        // 无效令牌拒绝
        assert!(list_repos(st.clone(), "bad-token".into()).is_err());

        drop(st);
        drop(app);
        std::fs::remove_dir_all(&root).ok();
    }
}
