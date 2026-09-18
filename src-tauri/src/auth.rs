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
    let conn = lock(&state.conn);
    let _ = conn.execute(
        "DELETE FROM sessions WHERE expires_at <= ?1",
        params![now],
    );
    conn.query_row(
        "SELECT username FROM sessions WHERE token = ?1 AND expires_at > ?2",
        params![token, now],
        |r| r.get::<_, String>(0),
    )
    .map_err(|_| "登录已失效，请重新登录".to_string())
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
        state.add_log("登录失败（用户名或密码错误）", &username);
        return Err("用户名或密码错误".into());
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
    state.add_log("登录", &username);
    Ok(LoginResult {
        token,
        username,
    })
}

#[tauri::command]
pub fn restore_session(state: State<'_, Arc<AppState>>, token: String) -> Result<String, String> {
    require_session(&state, &token)
}

#[tauri::command]
pub fn logout(state: State<'_, Arc<AppState>>, token: String) -> Result<(), String> {
    let username = require_session(&state, &token);
    {
        let conn = lock(&state.conn);
        let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", params![token]);
    }
    if let Ok(user) = username {
        state.add_log("退出登录", &user);
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
    let username = require_session(&state, &token)?;
    if new_password.chars().count() < 6 {
        return Err("新密码至少需要 6 个字符".into());
    }
    let conn = lock(&state.conn);
    let user = conn
        .query_row(
            "SELECT salt, password_hash FROM users WHERE username = ?1",
            params![username],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|_| "用户不存在".to_string())?;
    if hash_password(&user.0, &old_password) != user.1 {
        return Err("旧密码不正确".into());
    }
    let salt = Uuid::new_v4().simple().to_string();
    conn.execute(
        "UPDATE users SET salt = ?1, password_hash = ?2 WHERE username = ?3",
        params![salt, hash_password(&salt, &new_password), username],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);
    state.add_log("修改密码", &username);
    Ok(())
}
