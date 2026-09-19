use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

use crate::lang::Lang;

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
    pub last_synced: Option<i64>,
    pub last_status: String,
    pub last_message: Option<String>,
    /// 备份目标（除 origin 外的全部远端），1 对多
    #[serde(default)]
    pub targets: Vec<TargetState>,
}

/// 一个备份目标的状态（按远端名独立记录）
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetState {
    pub remote: String,
    pub url: String,
    pub last_status: String,
    pub last_message: Option<String>,
    pub last_synced: Option<i64>,
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

/// 同步队列中的一个任务（仓库串行处理，避免并发拉取抢占网络）
#[derive(Clone)]
pub struct SyncJob {
    pub repo_id: String,
    pub operator: String,
    pub lang: Lang,
}

#[derive(Default)]
pub struct SyncQueue {
    pub jobs: VecDeque<SyncJob>,
    pub worker_active: bool,
}

pub struct AppState {
    pub conn: Mutex<Connection>,
    pub sync_procs: Mutex<HashMap<String, Arc<SyncHandle>>>,
    pub syncing: Mutex<HashSet<String>>,
    pub stop_requested: Mutex<HashSet<String>>,
    pub sync_queue: Mutex<SyncQueue>,
}

/// 默认基地址：本机用户目录下的 repo 目录（完整路径）
pub fn default_base_dir() -> String {
    dirs::home_dir()
        .map(|h| h.join("repo").to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into())
}

/// 基地址统一以完整路径存储：以 ~ 开头的输入写入时展开（兼容旧数据与 MCP 入口）
pub fn normalize_base_dir(path: &str) -> String {
    let t = path.trim();
    if t == "~" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| t.into());
    }
    if let Some(rest) = t.strip_prefix("~/").or_else(|| t.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    t.to_string()
}

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
            sync_queue: Mutex::new(SyncQueue::default()),
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
            CREATE TABLE IF NOT EXISTS sync_targets (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_id      TEXT NOT NULL,
                remote       TEXT NOT NULL,
                url          TEXT NOT NULL,
                last_synced  INTEGER,
                last_status  TEXT NOT NULL DEFAULT 'idle',
                last_message TEXT,
                UNIQUE(repo_id, remote)
            );
            CREATE TABLE IF NOT EXISTS repos (
                id           TEXT PRIMARY KEY,
                name         TEXT NOT NULL,
                source       TEXT NOT NULL,
                target       TEXT NOT NULL DEFAULT '',
                last_synced  INTEGER,
                last_status  TEXT NOT NULL DEFAULT 'idle',
                last_message TEXT,
                hidden       INTEGER NOT NULL DEFAULT 0
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
        // 旧库升级：repos 表补 hidden 列（已存在时忽略错误）
        let _ = conn.execute("ALTER TABLE repos ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0", []);
        // 旧库升级：单目标 target 列迁移到 sync_targets（UNIQUE 幂等），迁移后清空旧列
        let _ = conn.execute(
            "INSERT OR IGNORE INTO sync_targets (repo_id, remote, url)
             SELECT id, 'backup', target FROM repos WHERE target != ''",
            [],
        );
        let _ = conn.execute("UPDATE repos SET target = '' WHERE target != ''", []);
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
            self.set_setting("base_dir", &default_base_dir());
        } else {
            // 存量数据迁移：~ 开头的旧值归一化为完整路径
            let v = self.get_setting("base_dir").unwrap_or_default();
            let n = normalize_base_dir(&v);
            if n != v {
                self.set_setting("base_dir", &n);
            }
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

/// 为仓库列表附加备份目标状态
pub fn attach_targets(conn: &Connection, repos: &mut [Repo]) {
    for r in repos.iter_mut() {
        r.targets.clear();
    }
    let mut stmt = match conn.prepare(
        "SELECT repo_id, remote, url, last_status, last_message, last_synced
         FROM sync_targets ORDER BY remote",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            TargetState {
                remote: row.get(1)?,
                url: row.get(2)?,
                last_status: row.get(3)?,
                last_message: row.get(4)?,
                last_synced: row.get(5)?,
            },
        ))
    }) {
        Ok(r) => r,
        Err(_) => return,
    };
    let map: std::collections::HashMap<String, Vec<TargetState>> = rows
        .filter_map(Result::ok)
        .fold(std::collections::HashMap::new(), |mut m, (id, t)| {
            m.entry(id).or_default().push(t);
            m
        });
    for r in repos.iter_mut() {
        if let Some(ts) = map.get(&r.id) {
            r.targets = ts.clone();
        }
    }
}
