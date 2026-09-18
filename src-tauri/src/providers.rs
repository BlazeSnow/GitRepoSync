use crate::auth::require_session;
use crate::state::{lock, AppState};
use rusqlite::params;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub platform: String,
    pub has_pat: bool,
    pub pat_masked: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgInfo {
    pub login: String,
    pub name: String,
    pub avatar_url: String,
    pub description: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub platform: String,
    pub login: String,
    pub name: String,
    pub avatar_url: String,
    pub orgs: Vec<OrgInfo>,
}

fn mask(pat: &str) -> String {
    let chars: Vec<char> = pat.chars().collect();
    if chars.len() <= 8 {
        "••••".into()
    } else {
        format!(
            "{}••••{}",
            chars[..4].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        )
    }
}

fn get_pat(state: &AppState, platform: &str) -> Option<String> {
    let conn = lock(&state.conn);
    conn.query_row(
        "SELECT pat FROM providers WHERE platform = ?1",
        params![platform],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

#[tauri::command]
pub fn get_providers(
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<Vec<ProviderInfo>, String> {
    require_session(&state, &token)?;
    Ok(["github", "gitlab"]
        .iter()
        .map(|p| match get_pat(&state, p) {
            Some(pat) => ProviderInfo {
                platform: p.to_string(),
                has_pat: true,
                pat_masked: Some(mask(&pat)),
            },
            None => ProviderInfo {
                platform: p.to_string(),
                has_pat: false,
                pat_masked: None,
            },
        })
        .collect())
}

#[tauri::command]
pub fn save_provider(
    state: State<'_, Arc<AppState>>,
    token: String,
    platform: String,
    pat: Option<String>,
) -> Result<(), String> {
    let username = require_session(&state, &token)?;
    let pat = pat.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    match platform.as_str() {
        "github" | "gitlab" => {}
        _ => return Err("不支持的平台".into()),
    }
    {
        let conn = lock(&state.conn);
        conn.execute(
            "INSERT INTO providers (platform, pat) VALUES (?1, ?2)
             ON CONFLICT(platform) DO UPDATE SET pat = excluded.pat",
            params![platform, pat],
        )
        .map_err(|e| e.to_string())?;
    }
    state.add_log(
        &match &pat {
            Some(_) => format!("保存 {platform} PAT"),
            None => format!("清除 {platform} PAT"),
        },
        &username,
    );
    Ok(())
}

#[tauri::command]
pub async fn fetch_provider_accounts(
    state: State<'_, Arc<AppState>>,
    token: String,
    platform: String,
) -> Result<AccountInfo, String> {
    require_session(&state, &token)?;
    let pat = get_pat(&state, &platform).ok_or("请先保存该平台的 PAT")?;

    match platform.as_str() {
        "github" => fetch_github(&platform, &pat).await,
        "gitlab" => fetch_gitlab(&platform, &pat).await,
        _ => Err("不支持的平台".into()),
    }
}

fn json_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn api_err(e: reqwest::Error, platform: &str) -> String {
    if e.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
        "PAT 无效或已过期".into()
    } else {
        format!("{platform} API 请求失败：{e}")
    }
}

async fn fetch_github(platform: &str, pat: &str) -> Result<AccountInfo, String> {
    let client = reqwest::Client::new();
    let build = |url: &str| {
        client
            .get(url)
            .header("Authorization", format!("Bearer {pat}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "GitRepoSync")
            .header("X-GitHub-Api-Version", "2022-11-28")
    };

    let user: serde_json::Value = build("https://api.github.com/user")
        .send()
        .await
        .map_err(|e| api_err(e, platform))?
        .error_for_status()
        .map_err(|e| api_err(e, platform))?
        .json()
        .await
        .map_err(|e| format!("解析 GitHub 响应失败：{e}"))?;

    let orgs_raw: serde_json::Value = build("https://api.github.com/user/orgs")
        .send()
        .await
        .map_err(|e| api_err(e, platform))?
        .error_for_status()
        .map_err(|e| api_err(e, platform))?
        .json()
        .await
        .map_err(|e| format!("解析 GitHub 响应失败：{e}"))?;

    let mut orgs = Vec::new();
    if let Some(arr) = orgs_raw.as_array() {
        for o in arr {
            orgs.push(OrgInfo {
                login: json_str(o, "login"),
                name: json_str(o, "login"),
                avatar_url: json_str(o, "avatar_url"),
                description: json_str(o, "description"),
            });
        }
    }

    let login = json_str(&user, "login");
    let name = {
        let n = json_str(&user, "name");
        if n.is_empty() {
            login.clone()
        } else {
            n
        }
    };
    Ok(AccountInfo {
        platform: platform.into(),
        login,
        name,
        avatar_url: json_str(&user, "avatar_url"),
        orgs,
    })
}

async fn fetch_gitlab(platform: &str, pat: &str) -> Result<AccountInfo, String> {
    let client = reqwest::Client::new();
    let base = "https://gitlab.com/api/v4";
    let build = |url: String| {
        client
            .get(url)
            .header("PRIVATE-TOKEN", pat)
            .header("User-Agent", "GitRepoSync")
    };

    let user: serde_json::Value = build(format!("{base}/user"))
        .send()
        .await
        .map_err(|e| api_err(e, platform))?
        .error_for_status()
        .map_err(|e| api_err(e, platform))?
        .json()
        .await
        .map_err(|e| format!("解析 GitLab 响应失败：{e}"))?;

    let groups_raw: serde_json::Value = build(format!(
        "{base}/groups?per_page=100&min_access_role=10"
    ))
    .send()
    .await
    .map_err(|e| api_err(e, platform))?
    .error_for_status()
    .map_err(|e| api_err(e, platform))?
    .json()
    .await
    .map_err(|e| format!("解析 GitLab 响应失败：{e}"))?;

    let mut orgs = Vec::new();
    if let Some(arr) = groups_raw.as_array() {
        for g in arr {
            orgs.push(OrgInfo {
                login: json_str(g, "path"),
                name: json_str(g, "name"),
                avatar_url: json_str(g, "avatar_url"),
                description: json_str(g, "description"),
            });
        }
    }

    let login = json_str(&user, "username");
    let name = {
        let n = json_str(&user, "name");
        if n.is_empty() {
            login.clone()
        } else {
            n
        }
    };
    Ok(AccountInfo {
        platform: platform.into(),
        login,
        name,
        avatar_url: json_str(&user, "avatar_url"),
        orgs,
    })
}
