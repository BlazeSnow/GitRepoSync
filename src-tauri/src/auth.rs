use crate::state::{lock, save_json, AppState, Session};
use chrono::Utc;
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
    let mut found: Option<String> = None;
    {
        let mut sessions = lock(&state.sessions);
        sessions.retain(|s| s.expires_at > now);
        if let Some(s) = sessions.iter().find(|s| s.token == token) {
            found = Some(s.username.clone());
        }
    }
    match found {
        Some(username) => {
            let _ = save_json(&state.sessions_path(), &*lock(&state.sessions));
            Ok(username)
        }
        None => Err("登录已失效，请重新登录".into()),
    }
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
    let user = {
        let users = lock(&state.users);
        users
            .iter()
            .find(|u| u.username == username)
            .cloned()
            .ok_or("用户名或密码错误")?
    };
    if hash_password(&user.salt, &password) != user.password_hash {
        return Err("用户名或密码错误".into());
    }
    let days: i64 = if remember.unwrap_or(false) { 30 } else { 1 };
    let token = Uuid::new_v4().to_string();
    let session = Session {
        token: token.clone(),
        username: user.username.clone(),
        expires_at: Utc::now().timestamp() + days * 86400,
    };
    {
        lock(&state.sessions).push(session);
        let _ = save_json(&state.sessions_path(), &*lock(&state.sessions));
    }
    Ok(LoginResult {
        token,
        username: user.username,
    })
}

#[tauri::command]
pub fn restore_session(state: State<'_, Arc<AppState>>, token: String) -> Result<String, String> {
    require_session(&state, &token)
}

#[tauri::command]
pub fn logout(state: State<'_, Arc<AppState>>, token: String) -> Result<(), String> {
    {
        lock(&state.sessions).retain(|s| s.token != token);
    }
    let _ = save_json(&state.sessions_path(), &*lock(&state.sessions));
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
    let mut users = lock(&state.users);
    let user = users
        .iter_mut()
        .find(|u| u.username == username)
        .ok_or("用户不存在")?;
    if hash_password(&user.salt, &old_password) != user.password_hash {
        return Err("旧密码不正确".into());
    }
    user.salt = Uuid::new_v4().simple().to_string();
    user.password_hash = hash_password(&user.salt, &new_password);
    save_json(&state.users_path(), &*users)?;
    Ok(())
}
