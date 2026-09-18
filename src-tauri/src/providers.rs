use crate::auth::require_session;
use crate::state::{lock, save_json, AppState};
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

#[tauri::command]
pub fn get_providers(
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<Vec<ProviderInfo>, String> {
    require_session(&state, &token)?;
    let p = lock(&state.providers);
    Ok(vec![
        ProviderInfo {
            platform: "github".into(),
            has_pat: p.github.is_some(),
            pat_masked: p.github.as_ref().map(|s| mask(s)),
        },
        ProviderInfo {
            platform: "gitlab".into(),
            has_pat: p.gitlab.is_some(),
            pat_masked: p.gitlab.as_ref().map(|s| mask(s)),
        },
    ])
}

#[tauri::command]
pub fn save_provider(
    state: State<'_, Arc<AppState>>,
    token: String,
    platform: String,
    pat: Option<String>,
) -> Result<(), String> {
    require_session(&state, &token)?;
    let pat = pat.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    {
        let mut p = lock(&state.providers);
        match platform.as_str() {
            "github" => p.github = pat,
            "gitlab" => p.gitlab = pat,
            _ => return Err("不支持的平台".into()),
        }
    }
    let path = state.data_dir.join("providers.json");
    save_json(&path, &*lock(&state.providers))
}

#[tauri::command]
pub async fn fetch_provider_accounts(
    state: State<'_, Arc<AppState>>,
    token: String,
    platform: String,
) -> Result<AccountInfo, String> {
    require_session(&state, &token)?;
    let pat = {
        let p = lock(&state.providers);
        match platform.as_str() {
            "github" => p.github.clone(),
            "gitlab" => p.gitlab.clone(),
            _ => return Err("不支持的平台".into()),
        }
    }
    .ok_or("请先保存该平台的 PAT")?;

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
