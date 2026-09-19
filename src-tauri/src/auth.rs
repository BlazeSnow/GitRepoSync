use crate::lang::{gui_lang, tr};
use crate::state::{lock, AppState};
use chrono::Utc;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

pub fn hash_password(salt: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// 校验会话令牌，过期会话顺手清理；成功返回用户名
pub fn require_session(state: &AppState, token: &str) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let lang = gui_lang();
    let conn = lock(&state.conn);
    let _ = conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", params![now]);
    conn.query_row(
        "SELECT username FROM sessions WHERE token = ?1 AND expires_at > ?2",
        params![token, now],
        |r| r.get::<_, String>(0),
    )
    .map_err(|_| tr(lang, "session-expired"))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    pub token: String,
    pub username: String,
}

#[tauri::command]
pub fn login(
    state: State<'_, Arc<AppState>>,
    username: String,
    password: String,
    remember: Option<bool>,
) -> Result<LoginResult, String> {
    let lang = gui_lang();
    let authed = {
        let conn = lock(&state.conn);
        conn.query_row(
            "SELECT salt, password_hash FROM users WHERE username = ?1",
            params![username],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map(|(salt, hash)| hash_password(&salt, &password) == hash)
        .unwrap_or(false)
    };
    if !authed {
        state.add_log(&tr(lang, "login-failed-log"), &username);
        return Err(tr(lang, "invalid-credentials"));
    }

    let days: i64 = if remember.unwrap_or(false) { 30 } else { 1 };
    let token = Uuid::new_v4().to_string();
    {
        let conn = lock(&state.conn);
        conn.execute(
            "INSERT INTO sessions (token, username, expires_at) VALUES (?1, ?2, ?3)",
            params![token, username, Utc::now().timestamp() + days * 86400],
        )
        .map_err(|e| e.to_string())?;
    }
    state.add_log(&tr(lang, "log-login"), &username);
    Ok(LoginResult { token, username })
}

#[tauri::command]
pub fn restore_session(state: State<'_, Arc<AppState>>, token: String) -> Result<String, String> {
    require_session(&state, &token)
}

#[tauri::command]
pub fn logout(state: State<'_, Arc<AppState>>, token: String) -> Result<(), String> {
    let lang = gui_lang();
    let username = require_session(&state, &token);
    {
        let conn = lock(&state.conn);
        let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", params![token]);
    }
    if let Ok(user) = username {
        state.add_log(&tr(lang, "log-logout"), &user);
    }
    Ok(())
}

#[tauri::command]
pub fn change_password(
    state: State<'_, Arc<AppState>>,
    token: String,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    let lang = gui_lang();
    let username = require_session(&state, &token)?;
    if new_password.chars().count() < 6 {
        return Err(tr(lang, "new-password-too-short"));
    }
    let conn = lock(&state.conn);
    let user = conn
        .query_row(
            "SELECT salt, password_hash FROM users WHERE username = ?1",
            params![username],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| tr(lang, "user-not-found"))?;
    if hash_password(&user.0, &old_password) != user.1 {
        return Err(tr(lang, "wrong-password"));
    }
    let salt = Uuid::new_v4().simple().to_string();
    conn.execute(
        "UPDATE users SET salt = ?1, password_hash = ?2 WHERE username = ?3",
        params![salt, hash_password(&salt, &new_password), username],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);
    state.add_log(&tr(lang, "log-change-password"), &username);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 升级 sha2 大版本后校验实现符合标准向量（参考值由 Node crypto 生成）
    #[test]
    fn hash_matches_reference() {
        assert_eq!(
            hash_password("00", "admin123"),
            "8ebeceffd4c60f2e15bb5bda22116ab3b6cebdee20f39422be7ccfacddceada2"
        );
    }

    /// 会话校验：过期会话被拒绝并顺手清理，有效会话返回用户名
    #[test]
    fn require_session_expires_and_cleans_up() {
        use crate::state::AppState;

        let root =
            std::env::temp_dir().join(format!("grs-auth-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::open(root.join("app.db")).unwrap();
        let now = Utc::now().timestamp();
        {
            let conn = lock(&state.conn);
            conn.execute(
                "INSERT INTO sessions (token, username, expires_at) VALUES ('expired', 'admin', ?1)",
                params![now - 10],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sessions (token, username, expires_at) VALUES ('valid', 'admin', ?1)",
                params![now + 3600],
            )
            .unwrap();
        }
        assert!(require_session(&state, "expired").is_err(), "过期会话被拒绝");
        let remaining: i64 = {
            let conn = lock(&state.conn);
            conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE token = 'expired'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(remaining, 0, "过期会话被顺手清理");
        assert_eq!(require_session(&state, "valid").unwrap(), "admin");
        assert!(require_session(&state, "nope").is_err(), "未知令牌被拒绝");
        drop(state);
        std::fs::remove_dir_all(&root).ok();
    }
}
