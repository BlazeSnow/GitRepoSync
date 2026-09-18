use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

/// std::sync::Mutex 带毒恢复锁：后台线程持有锁时 panic 不应拖垮整个应用
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub source: String,
    pub target: String,
    pub last_synced: Option<i64>,
    pub last_status: String,
    pub last_message: Option<String>,
}

/// 操作日志条目（软件全部操作入库 sqlite）
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLog {
    pub id: i64,
    pub action: String,
    pub operator: String,
    pub created_at: i64,
}

/// 一次同步对应的 git 子进程句柄，供“停止同步”终止进程
pub struct SyncHandle {
    pub child: Mutex<Option<std::process::Child>>,
}

pub struct AppState {
    pub conn: Mutex<Connection>,
    pub sync_procs: Mutex<HashMap<String, Arc<SyncHandle>>>,
    pub syncing: Mutex<HashSet<String>>,
    pub stop_requested: Mutex<HashSet<String>>,
}

pub const DEFAULT_BASE_DIR: &str = "~/repo";

impl AppState {
    /// 打开（或创建）sqlite 数据库并初始化表结构与默认数据
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建数据目录失败: {e}"))?;
        }
        let conn = Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("设置 WAL 失败: {e}"))?;
        Self::ensure_schema(&conn);

        let state = Self {
            conn: Mutex::new(conn),
            sync_procs: Mutex::new(HashMap::new()),
            syncing: Mutex::new(HashSet::new()),
            stop_requested: Mutex::new(HashSet::new()),
        };
        state.seed();
        Ok(state)
    }

    fn ensure_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                username      TEXT PRIMARY KEY,
                salt          TEXT NOT NULL,
                password_hash TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                token      TEXT PRIMARY KEY,
                username   TEXT NOT NULL,
                expires_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS repos (
                id           TEXT PRIMARY KEY,
                name         TEXT NOT NULL,
                source       TEXT NOT NULL,
                target       TEXT NOT NULL,
                last_synced  INTEGER,
                last_status  TEXT NOT NULL DEFAULT 'idle',
                last_message TEXT
            );
            CREATE TABLE IF NOT EXISTS providers (
                platform TEXT PRIMARY KEY,
                pat      TEXT
            );
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS operation_logs (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                action     TEXT NOT NULL,
                operator   TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );",
        )
        .expect("初始化数据库表失败");
    }

    /// 首次启动初始化：admin/admin123 用户、默认基地址、MCP APIKEY
    fn seed(&self) {
        {
            let conn = lock(&self.conn);
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
                .unwrap_or(0);
            if count == 0 {
                let salt = Uuid::new_v4().simple().to_string();
                let _ = conn.execute(
                    "INSERT INTO users (username, salt, password_hash) VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        "admin",
                        salt,
                        crate::auth::hash_password(&salt, "admin123")
                    ],
                );
            }
        }
        if self.get_setting("base_dir").is_none() {
            self.set_setting("base_dir", DEFAULT_BASE_DIR);
        }
        if self
            .get_setting("mcp_api_key")
            .is_none_or(|v| v.is_empty())
        {
            self.set_setting("mcp_api_key", &format!("grs_{}", Uuid::new_v4().simple()));
        }
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = lock(&self.conn);
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get(0),
        )
        .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let conn = lock(&self.conn);
        let _ = conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        );
    }

    /// 记录操作日志（软件全部操作入库）
    pub fn add_log(&self, action: &str, operator: &str) {
        let conn = lock(&self.conn);
        let _ = conn.execute(
            "INSERT INTO operation_logs (action, operator, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![action, operator, now_ms()],
        );
    }

    /// 将残留的 running 状态复位为 idle（仅应用启动时调用）
    pub fn reset_running_repos(&self) {
        let conn = lock(&self.conn);
        let _ = conn.execute(
            "UPDATE repos SET last_status = 'idle' WHERE last_status = 'running'",
            [],
        );
    }

    /// 等待所有在途同步结束（MCP 会话断开后保持进程存活，避免杀死同步）
    pub fn wait_syncs_idle(&self) {
        loop {
            if lock(&self.syncing).is_empty() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}
