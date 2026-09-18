use crate::auth::require_session;
use crate::lang::{gui_lang, tr, tr_a};
use crate::state::{lock, AppState};
use rusqlite::params;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
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

/// 主账号下的一个仓库（用于“未添加的仓库”列表）
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableRepo {
    pub platform: String,
    pub platform_id: i64,
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub description: String,
    pub private: bool,
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

/// 平台 API 专用客户端：网络异常时超时返回错误，而不是无限挂起
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("构建 HTTP 客户端失败")
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
        _ => return Err(tr(gui_lang(), "unsupported-platform")),
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
        &tr_a(
            gui_lang(),
            match &pat {
                Some(_) => "log-provider-saved",
                None => "log-provider-cleared",
            },
            &[("platform", &platform)],
        ),
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
    let pat = get_pat(&state, &platform).ok_or_else(|| tr(gui_lang(), "pat-missing"))?;

    match platform.as_str() {
        "github" => fetch_github(&platform, &pat).await,
        "gitlab" => fetch_gitlab(&platform, &pat).await,
        _ => Err(tr(gui_lang(), "unsupported-platform")),
    }
}

fn json_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn api_err(e: reqwest::Error, platform: &str) -> String {
    let lang = gui_lang();
    if e.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
        tr(lang, "pat-invalid")
    } else {
        tr_a(
            lang,
            "api-request-error",
            &[("platform", platform), ("err", &e.to_string())],
        )
    }
}

async fn fetch_github(platform: &str, pat: &str) -> Result<AccountInfo, String> {
    let client = http_client();
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
        .map_err(|e| tr_a(gui_lang(), "api-parse-error", &[("platform", platform), ("err", &e.to_string())]))?;

    let orgs_raw: serde_json::Value = build("https://api.github.com/user/orgs")
        .send()
        .await
        .map_err(|e| api_err(e, platform))?
        .error_for_status()
        .map_err(|e| api_err(e, platform))?
        .json()
        .await
        .map_err(|e| tr_a(gui_lang(), "api-parse-error", &[("platform", platform), ("err", &e.to_string())]))?;

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
    let client = http_client();
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
        .map_err(|e| tr_a(gui_lang(), "api-parse-error", &[("platform", platform), ("err", &e.to_string())]))?;

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
    .map_err(|e| tr_a(gui_lang(), "api-parse-error", &[("platform", platform), ("err", &e.to_string())]))?;

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

/// 规范化仓库 URL 用于“是否已添加”比对：
/// 去协议（https/http/ssh/git@）、去 .git 后缀、git@host:path 转 host/path、小写
fn norm_url(u: &str) -> String {
    let mut s = u.trim().to_lowercase();
    for prefix in ["https://", "http://", "ssh://", "git@"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    if let Some(rest) = s.strip_suffix(".git") {
        s = rest.to_string();
    }
    s = s.replace(':', "/");
    s.trim_end_matches('/').to_string()
}

/// 列出主账号下的仓库，并过滤掉已添加（源地址规范化后能对上）的仓库
#[tauri::command]
pub async fn list_available_repos(
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<Vec<AvailableRepo>, String> {
    require_session(&state, &token)?;
    let platform = match state.get_setting("primary_platform") {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(vec![]),
    };
    let pat = get_pat(&state, &platform).ok_or_else(|| tr(gui_lang(), "pat-missing"))?;

    let mut all = match platform.as_str() {
        "github" => fetch_github_repos(&platform, &pat).await?,
        "gitlab" => fetch_gitlab_repos(&platform, &pat).await?,
        _ => return Err(tr(gui_lang(), "unsupported-platform")),
    };

    let existing: Vec<String> = {
        let conn = lock(&state.conn);
        let mut stmt = conn
            .prepare("SELECT source FROM repos")
            .map_err(|e| e.to_string())?;
        let sources = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        sources
    };
    let existing: std::collections::HashSet<String> =
        existing.iter().map(|s| norm_url(s)).collect();

    all.retain(|r| !existing.contains(&norm_url(&r.clone_url)) && !existing.contains(&norm_url(&r.ssh_url)));
    Ok(all)
}

async fn fetch_github_repos(platform: &str, pat: &str) -> Result<Vec<AvailableRepo>, String> {
    let client = http_client();
    let raw: serde_json::Value = client
        .get("https://api.github.com/user/repos?per_page=100&type=owner&sort=full_name")
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "GitRepoSync")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| api_err(e, platform))?
        .error_for_status()
        .map_err(|e| api_err(e, platform))?
        .json()
        .await
        .map_err(|e| tr_a(gui_lang(), "api-parse-error", &[("platform", platform), ("err", &e.to_string())]))?;

    let mut repos = Vec::new();
    if let Some(arr) = raw.as_array() {
        for r in arr {
            repos.push(AvailableRepo {
                platform: platform.into(),
                platform_id: r.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                name: json_str(r, "name"),
                full_name: json_str(r, "full_name"),
                clone_url: json_str(r, "clone_url"),
                ssh_url: json_str(r, "ssh_url"),
                description: json_str(r, "description"),
                private: r.get("private").and_then(|v| v.as_bool()).unwrap_or(false),
            });
        }
    }
    Ok(repos)
}

async fn fetch_gitlab_repos(platform: &str, pat: &str) -> Result<Vec<AvailableRepo>, String> {
    let client = http_client();
    let raw: serde_json::Value = client
        .get("https://gitlab.com/api/v4/projects?owned=true&per_page=100&order_by=name")
        .header("PRIVATE-TOKEN", pat)
        .header("User-Agent", "GitRepoSync")
        .send()
        .await
        .map_err(|e| api_err(e, platform))?
        .error_for_status()
        .map_err(|e| api_err(e, platform))?
        .json()
        .await
        .map_err(|e| tr_a(gui_lang(), "api-parse-error", &[("platform", platform), ("err", &e.to_string())]))?;

    let mut repos = Vec::new();
    if let Some(arr) = raw.as_array() {
        for r in arr {
            repos.push(AvailableRepo {
                platform: platform.into(),
                platform_id: r.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                name: json_str(r, "path"),
                full_name: json_str(r, "path_with_namespace"),
                clone_url: json_str(r, "http_url_to_repo"),
                ssh_url: json_str(r, "ssh_url_to_repo"),
                description: json_str(r, "description"),
                private: json_str(r, "visibility") == "private",
            });
        }
    }
    Ok(repos)
}
