//! 仓库 CRUD 的 Tauri 命令：列表（含自动发现）、保存、删除。
//! 同步命令见 sync.rs；git 流水线见 git.rs；发现逻辑见 discover.rs。

use crate::auth::require_session;
use crate::discover::discover;
use crate::lang::{gui_lang, tr, tr_a};
use crate::state::{lock, repo_from_row, AppState, Repo, REPO_COLS};
use rusqlite::params;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

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

/// save_repo 的单个备份目标输入
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInput {
    pub remote: String,
    pub url: String,
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
    // 名称唯一：name 是中转目录名与自动发现的身份（数据库层有唯一索引兜底），
    // 此处先给出可读提示（含隐藏行，改名不得与任何现有行冲突）
    {
        let conn = lock(&state.conn);
        let dup: i64 = if let Some(rid) = &id {
            conn.query_row(
                "SELECT COUNT(*) FROM repos WHERE name = ?1 AND id != ?2",
                params![name, rid],
                |r| r.get(0),
            )
            .unwrap_or(0)
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM repos WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .unwrap_or(0)
        };
        if dup > 0 {
            return Err(tr_a(lang, "repo-name-exists", &[("name", &name)]));
        }
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

    /// 命令层：新增（目标清洗）、重名拒绝、编辑整体替换目标、软删除后列表不可见、无效令牌拒绝
    #[test]
    fn save_list_delete_repo_commands() {
        use tauri::Manager;
        let (app, _state, root) = open_mock_app("repos");
        let st = app.state::<Arc<AppState>>();
        insert_session(_state.as_ref(), "tok");

        // 新增：空目标行被清洗，列表可见且目标已附加
        let repo = save_repo(
            st.clone(),
            "tok".into(),
            None,
            "demo".into(),
            "https://src/demo.git".into(),
            vec![
                TargetInput { remote: "backup".into(), url: "https://bak/demo.git".into() },
                TargetInput { remote: "  ".into(), url: "https://ignored.git".into() },
            ],
        )
        .unwrap();
        let list = list_repos(st.clone(), "tok".into()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].targets.len(), 1, "空白目标行应被清洗");
        assert_eq!(list[0].targets[0].remote, "backup");

        // 重名（含与其他可见行重名）拒绝
        assert!(save_repo(
            st.clone(),
            "tok".into(),
            None,
            "demo".into(),
            "https://src/other.git".into(),
            vec![],
        )
        .is_err());

        // 编辑：改名并整体替换目标列表
        let updated = save_repo(
            st.clone(),
            "tok".into(),
            Some(repo.id.clone()),
            "demo2".into(),
            "https://src/demo.git".into(),
            vec![
                TargetInput { remote: "a".into(), url: "https://a.git".into() },
                TargetInput { remote: "b".into(), url: "https://b.git".into() },
            ],
        )
        .unwrap();
        assert_eq!(updated.name, "demo2");
        let list = list_repos(st.clone(), "tok".into()).unwrap();
        assert_eq!(list[0].targets.len(), 2, "编辑应整体替换目标");

        // 软删除：列表不可见；重复删除幂等成功（行仍存在只是隐藏）
        delete_repo(st.clone(), "tok".into(), repo.id.clone()).unwrap();
        assert!(list_repos(st.clone(), "tok".into()).unwrap().is_empty());
        delete_repo(st.clone(), "tok".into(), repo.id).unwrap();

        // 无效令牌拒绝
        assert!(list_repos(st.clone(), "bad-token".into()).is_err());

        drop(st);
        drop(app);
        std::fs::remove_dir_all(&root).ok();
    }
}
