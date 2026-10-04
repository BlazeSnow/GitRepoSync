//! 仓库的 Tauri 命令：列表（含自动发现）与打开目录。
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

    /// list_repos：隐藏仓库不出现；无效令牌拒绝（删除/编辑入口已随
    /// 仓库代管一起移除，列表为纯只读视图）
    #[test]
    fn list_repos_excludes_hidden_and_requires_session() {
        use tauri::Manager;

        let (app, state, root) = open_mock_app("repos");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO repos (id, name, source, hidden) VALUES ('r1', 'demo', 'https://src/demo.git', 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO repos (id, name, source, hidden) VALUES ('r2', 'hidden-repo', 'https://src/h.git', 1)",
                [],
            )
            .unwrap();
        }
        let repos = list_repos(st.clone(), "tok".into()).unwrap();
        assert_eq!(repos.len(), 1, "隐藏仓库不应出现在列表");
        assert_eq!(repos[0].name, "demo");
        assert!(list_repos(st.clone(), "bad-token".into()).is_err(), "无效令牌应拒绝");

        drop(st);
        drop(app);
        std::fs::remove_dir_all(&root).ok();
    }
}
