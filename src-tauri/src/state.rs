use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

/// std::sync::Mutex 带毒恢复锁：后台线程持有锁时 panic 不应拖垮整个应用
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let s = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, s).map_err(|e| e.to_string())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub salt: String,
    pub password_hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub token: String,
    pub username: String,
    pub expires_at: i64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub source: String,
    pub target: String,
    pub last_synced: Option<i64>,
    pub last_status: String,
    pub last_message: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Providers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub mcp_api_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mcp_api_key: String::new(),
        }
    }
}

/// 一次同步对应的 git 子进程句柄，供“停止同步”终止进程
pub struct SyncHandle {
    pub child: Mutex<Option<std::process::Child>>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub users: Mutex<Vec<User>>,
    pub sessions: Mutex<Vec<Session>>,
    pub repos: Mutex<Vec<Repo>>,
    pub providers: Mutex<Providers>,
    pub settings: Mutex<Settings>,
    pub sync_procs: Mutex<HashMap<String, Arc<SyncHandle>>>,
    pub syncing: Mutex<HashSet<String>>,
}

impl AppState {
    pub fn init(data_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;

        // 首次启动时创建初始用户 admin / admin123
        let users_path = data_dir.join("users.json");
        let mut users: Vec<User> = load_json(&users_path).unwrap_or_default();
        if users.is_empty() {
            let salt = Uuid::new_v4().simple().to_string();
            users.push(User {
                username: "admin".into(),
                salt: salt.clone(),
                password_hash: crate::auth::hash_password(&salt, "admin123"),
            });
            save_json(&users_path, &users)?;
        }

        // 上次退出时残留的 running 状态复位为 idle
        let repos_path = data_dir.join("repos.json");
        let mut repos: Vec<Repo> = load_json(&repos_path).unwrap_or_default();
        let mut dirty = false;
        for r in repos.iter_mut() {
            if r.last_status == "running" {
                r.last_status = "idle".into();
                dirty = true;
            }
        }
        if dirty {
            save_json(&repos_path, &repos)?;
        }

        let settings_path = data_dir.join("settings.json");
        let mut settings: Settings = load_json(&settings_path).unwrap_or_default();
        if settings.mcp_api_key.is_empty() {
            settings.mcp_api_key = format!("grs_{}", Uuid::new_v4().simple());
            save_json(&settings_path, &settings)?;
        }
        Ok(Self {
            sessions: Mutex::new(load_json(&data_dir.join("sessions.json")).unwrap_or_default()),
            repos: Mutex::new(repos),
            providers: Mutex::new(load_json(&data_dir.join("providers.json")).unwrap_or_default()),
            settings: Mutex::new(settings),
            sync_procs: Mutex::new(HashMap::new()),
            syncing: Mutex::new(HashSet::new()),
            users: Mutex::new(users),
            data_dir,
        })
    }

    pub fn users_path(&self) -> PathBuf {
        self.data_dir.join("users.json")
    }

    pub fn sessions_path(&self) -> PathBuf {
        self.data_dir.join("sessions.json")
    }

    pub fn repos_path(&self) -> PathBuf {
        self.data_dir.join("repos.json")
    }

    pub fn save_repos(&self) -> Result<(), String> {
        save_json(&self.repos_path(), &*lock(&self.repos))
    }

    pub fn save_settings(&self) -> Result<(), String> {
        save_json(&self.data_dir.join("settings.json"), &*lock(&self.settings))
    }
}
