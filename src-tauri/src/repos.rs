//! 仓库的 Tauri 命令：列出（实时扫描基地址得到的真实列表）与打开目录。
//! 仓库配置只读：源与目标由 .git/config 派生（origin 为源、其余非
//! upstream 远端为目标），由用户自行用 git remote 管理，软件不代管。
//! 同步任务见 sync.rs；扫描逻辑见 scan.rs。

use crate::auth::require_session;
use crate::lang::{gui_lang, tr_a, Lang};
use crate::scan;
use crate::state::{AppState, Repo};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

/// 列出仓库：实时扫描基地址得到的真实列表（物理目录在就在，目录消失即消失）
#[tauri::command]
pub fn list_repos(state: State<'_, Arc<AppState>>, token: String) -> Result<Vec<Repo>, String> {
    require_session(&state, &token)?;
    Ok(scan::list_repos(state.inner()))
}

/// 解析仓库的本地中转目录（{基地址}/{仓库名}）并检查存在性；
/// 独立成函数便于单测（不实际拉起文件管理器）
fn resolve_repo_dir(state: &AppState, name: &str, lang: Lang) -> Result<PathBuf, String> {
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
    name: String,
) -> Result<(), String> {
    require_session(&state, &token)?;
    let dir = resolve_repo_dir(state.inner(), &name, gui_lang())?;
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

    /// open_repo_dir 的目录解析：存在返回路径、缺失报错（只测解析，不实际拉起文件管理器）
    #[test]
    fn resolve_repo_dir_checks_existence() {
        let (app, state, root) = open_mock_app("opendir");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("demo")).unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());
        assert_eq!(
            resolve_repo_dir(&state, "demo", crate::lang::Lang::Zh).unwrap(),
            base.join("demo")
        );
        // 目录缺失（尚未克隆）报错
        std::fs::remove_dir_all(base.join("demo")).unwrap();
        assert!(resolve_repo_dir(&state, "demo", crate::lang::Lang::Zh).is_err());

        drop(app);
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// list_repos：返回扫描得到的真实列表；无效令牌拒绝
    #[test]
    fn list_repos_requires_session() {
        use tauri::Manager;

        let (app, state, root) = open_mock_app("repos");
        let st = app.state::<Arc<AppState>>();
        insert_session(state.as_ref(), "tok");
        // 基地址下一个 git 仓库
        let base = root.join("base");
        std::fs::create_dir_all(base.join("demo/.git")).unwrap();
        std::fs::write(
            base.join("demo/.git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/u/demo.git\n",
        )
        .unwrap();
        state.set_setting("base_dir", &base.to_string_lossy());

        let repos = list_repos(st.clone(), "tok".into()).unwrap();
        assert_eq!(repos.len(), 1, "扫描到基地址内的 git 仓库");
        assert_eq!(repos[0].name, "demo");
        assert_eq!(repos[0].source, "https://github.com/u/demo.git");
        assert!(list_repos(st.clone(), "bad-token".into()).is_err(), "无效令牌应拒绝");

        drop(st);
        drop(app);
        std::fs::remove_dir_all(&root).ok();
    }
}
