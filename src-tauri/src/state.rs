use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, RwLock};
use std::time::Duration;
use uuid::Uuid;

use crate::sync::SyncEvent;

/// std::sync::Mutex 带毒恢复锁：后台线程持有锁时 panic 不应拖垮整个应用
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 仓库视图：身份为基地址下的目录名。源地址与备份目标实时扫描
/// `.git/config` 派生（config 即唯一真源），数据库只保存同步状态
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
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

/// 仓库级同步状态（repo_state 表的一行）
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub last_status: String,
    pub last_message: Option<String>,
    pub last_synced: Option<i64>,
}

impl Default for RepoState {
    fn default() -> Self {
        Self {
            last_status: "idle".to_string(),
            last_message: None,
            last_synced: None,
        }
    }
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

/// 守护角色单执行者：持有 sync-daemon.lock 的进程是唯一的同步执行者，
/// GUI 与 MCP 通过 SQLite 任务表（sync_jobs）投递、停止标记表（sync_stop）
/// 跨进程传递停止请求；执行者退出（进程崩溃 / 正常关闭）由 OS 释放锁，
/// 存活进程在下次投递或后台轮询时接管（try_become_daemon，见 sync.rs）。
pub struct AppState {
    pub conn: Mutex<Connection>,
    /// 守护角色文件锁句柄（Some = 本进程是唯一的同步执行者）
    pub(crate) daemon: Mutex<Option<std::fs::File>>,
    pub(crate) daemon_lock_path: PathBuf,
    /// sync-status 事件回调（GUI 注入 Tauri emit；MCP 与测试为空）
    emitter: RwLock<Option<Box<dyn Fn(SyncEvent) + Send + Sync>>>,
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
        // 多进程共用数据库（GUI 与 mcp 子命令）：写冲突时等待而非立即报错
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| format!("设置 busy_timeout 失败: {e}"))?;
        Self::ensure_schema(&conn);

        let state = Self {
            conn: Mutex::new(conn),
            daemon: Mutex::new(None),
            daemon_lock_path: db_path
                .parent()
                .map(|p| p.join("sync-daemon.lock"))
                .unwrap_or_else(|| PathBuf::from("sync-daemon.lock")),
            emitter: RwLock::new(None),
        };
        state.seed();
        Ok(state)
    }

    /// 本进程是否为同步执行者（守护角色）
    pub fn is_daemon(&self) -> bool {
        lock(&self.daemon).is_some()
    }

    /// 同步停止请求（跨进程）：stop_syncs 写入 sync_stop 表，执行者进程的
    /// git 轮询与流水线步骤边界自查后终止
    pub fn stop_requested(&self, repo_name: &str) -> bool {
        let conn = lock(&self.conn);
        conn.query_row(
            "SELECT 1 FROM sync_stop WHERE repo_name = ?1",
            params![repo_name],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// 注入 sync-status 事件回调（GUI 启动时注入 Tauri emit）
    pub fn set_emitter(&self, f: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        *self.emitter.write().unwrap_or_else(|p| p.into_inner()) = Some(f);
    }

    /// 推送同步状态事件（未注入回调时空操作）
    pub fn emit(&self, ev: SyncEvent) {
        if let Some(f) = self
            .emitter
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            f(ev);
        }
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
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS operation_logs (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                action     TEXT NOT NULL,
                operator   TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            -- 仓库同步状态：身份为基地址下的目录名；仓库配置不入库
            -- （源地址与备份目标每次扫描 .git/config 实时派生，config 即唯一真源）
            CREATE TABLE IF NOT EXISTS repo_state (
                name         TEXT PRIMARY KEY,
                last_synced  INTEGER,
                last_status  TEXT NOT NULL DEFAULT 'idle',
                last_message TEXT
            );
            -- 目标级同步状态：按（目录名, 远端名）关联
            CREATE TABLE IF NOT EXISTS target_state (
                repo_name    TEXT NOT NULL,
                remote       TEXT NOT NULL,
                last_synced  INTEGER,
                last_status  TEXT NOT NULL DEFAULT 'idle',
                last_message TEXT,
                UNIQUE(repo_name, remote)
            );
            CREATE TABLE IF NOT EXISTS sync_jobs (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_name  TEXT NOT NULL,
                operator   TEXT NOT NULL,
                lang       TEXT NOT NULL,
                state      TEXT NOT NULL DEFAULT 'queued',
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sync_stop (
                repo_name TEXT PRIMARY KEY
            );",
        )
        .expect("初始化数据库表失败");

        // 旧库升级：repos / sync_targets（含 id、source、target、hidden）已废弃——
        // 仓库配置改为实时扫描派生，这里只把同步状态迁入新表
        migrate_legacy_tables(conn);

        // 同仓库同时至多一个排队/运行中任务：跨进程投递的原子去重
        let _ = conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS uq_sync_jobs_active
             ON sync_jobs (repo_name) WHERE state IN ('queued', 'running')",
            [],
        );
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

    /// 等待所有在途同步结束（MCP 会话断开后：执行者进程等待 running 任务
    /// 完成再退出，避免杀死同步；客户端进程立即返回。排队任务留在表中，
    /// 由存活的执行者或下次接管继续）
    pub fn wait_syncs_idle(&self) {
        loop {
            if !self.is_daemon() {
                return;
            }
            let running: i64 = {
                let conn = lock(&self.conn);
                conn.query_row(
                    "SELECT COUNT(*) FROM sync_jobs WHERE state = 'running'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0)
            };
            if running == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// 旧库升级：repos / sync_targets 已废弃（仓库配置改为实时扫描 `.git/config`
/// 派生，不再入库），这里只把同步状态迁入 repo_state / target_state，并把
/// 任务队列与停止标记的关联键由 repo_id 换成目录名；迁移完成后丢弃旧表。
/// 迁移幂等：旧表不存在即视为已迁移。
fn migrate_legacy_tables(conn: &Connection) {
    let table_exists = |name: &str| -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0
    };
    if !table_exists("repos") {
        return; // 新库或已完成迁移
    }
    // 仓库级状态
    let _ = conn.execute(
        "INSERT OR REPLACE INTO repo_state (name, last_synced, last_status, last_message)
         SELECT name, last_synced, last_status, last_message FROM repos",
        [],
    );
    // 目标级状态：旧表按 repo_id 关联，迁移为按目录名关联
    let _ = conn.execute(
        "INSERT OR REPLACE INTO target_state (repo_name, remote, last_synced, last_status, last_message)
         SELECT r.name, t.remote, t.last_synced, t.last_status, t.last_message
         FROM sync_targets t JOIN repos r ON r.id = t.repo_id",
        [],
    );
    // 队列与停止标记：先把 repo_id 的值换成目录名（丢弃查不到对应仓库的孤儿行）
    let _ = conn.execute(
        "DELETE FROM sync_jobs WHERE repo_id NOT IN (SELECT id FROM repos)",
        [],
    );
    let _ = conn.execute(
        "DELETE FROM sync_stop WHERE repo_id NOT IN (SELECT id FROM repos)",
        [],
    );
    let _ = conn.execute(
        "UPDATE sync_jobs SET repo_id = (SELECT r.name FROM repos r WHERE r.id = sync_jobs.repo_id)",
        [],
    );
    let _ = conn.execute(
        "UPDATE sync_stop SET repo_id = (SELECT r.name FROM repos r WHERE r.id = sync_stop.repo_id)",
        [],
    );
    // 列名同步：repo_id → repo_name
    let _ = conn.execute("ALTER TABLE sync_jobs RENAME COLUMN repo_id TO repo_name", []);
    let _ = conn.execute("ALTER TABLE sync_stop RENAME COLUMN repo_id TO repo_name", []);
    // 丢弃旧表
    let _ = conn.execute("DROP TABLE IF EXISTS repos", []);
    let _ = conn.execute("DROP TABLE IF EXISTS sync_targets", []);
}

/// 读取全部仓库级同步状态（目录名 → 状态）
pub fn load_repo_states(conn: &Connection) -> HashMap<String, RepoState> {
    let mut stmt =
        match conn.prepare("SELECT name, last_status, last_message, last_synced FROM repo_state") {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            RepoState {
                last_status: r.get(1)?,
                last_message: r.get(2)?,
                last_synced: r.get(3)?,
            },
        ))
    });
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => HashMap::new(),
    }
}

/// 读取全部目标级同步状态（（目录名, 远端名）→ 状态）
pub fn load_target_states(
    conn: &Connection,
) -> HashMap<(String, String), (String, Option<String>, Option<i64>)> {
    let mut stmt = match conn.prepare(
        "SELECT repo_name, remote, last_status, last_message, last_synced FROM target_state",
    ) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
            (r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<i64>>(4)?),
        ))
    });
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ~ 开头的输入展开为完整路径，其余去空白后原样保存
    #[test]
    fn normalize_base_dir_expands_tilde() {
        let home = dirs::home_dir().expect("测试环境无用户目录");
        assert_eq!(normalize_base_dir("~"), home.to_string_lossy());
        assert_eq!(
            normalize_base_dir("~/repo/sub"),
            home.join("repo/sub").to_string_lossy()
        );
        assert_eq!(
            normalize_base_dir("~\\repo"),
            home.join("repo").to_string_lossy(),
            "Windows 风格 ~\\ 同样展开"
        );
        assert_eq!(normalize_base_dir("  /tmp/x  "), "/tmp/x", "去首尾空白");
        assert_eq!(normalize_base_dir("C:\\repo"), "C:\\repo", "普通路径原样");
    }

    /// 旧库升级：repos / sync_targets 的同步状态迁入 repo_state / target_state，
    /// 任务队列与停止标记的关联键由 repo_id 换成目录名，旧表被丢弃；迁移幂等
    #[test]
    fn legacy_tables_are_migrated_on_open() {
        let root =
            std::env::temp_dir().join(format!("grs-migrate-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("app.db");
        // 手工构造旧库
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE repos (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, source TEXT NOT NULL,
                    target TEXT NOT NULL DEFAULT '', last_synced INTEGER,
                    last_status TEXT NOT NULL DEFAULT 'idle', last_message TEXT,
                    hidden INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE sync_targets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, repo_id TEXT NOT NULL,
                    remote TEXT NOT NULL, url TEXT NOT NULL, last_synced INTEGER,
                    last_status TEXT NOT NULL DEFAULT 'idle', last_message TEXT,
                    UNIQUE(repo_id, remote));
                CREATE TABLE sync_jobs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, repo_id TEXT NOT NULL,
                    operator TEXT NOT NULL, lang TEXT NOT NULL,
                    state TEXT NOT NULL DEFAULT 'queued', created_at INTEGER NOT NULL);
                CREATE TABLE sync_stop (repo_id TEXT PRIMARY KEY);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO repos (id, name, source, last_synced, last_status, last_message)
                 VALUES ('a1', 'alpha', 'https://src/alpha.git', 1000, 'success', 'ok')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_targets (repo_id, remote, url, last_synced, last_status)
                 VALUES ('a1', 'gitlab', 'https://gl/alpha.git', 2000, 'failed')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_jobs (repo_id, operator, lang, state, created_at)
                 VALUES ('a1', 'admin', 'zh', 'queued', 3000)",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO sync_stop (repo_id) VALUES ('a1')", []).unwrap();
        }
        let state = AppState::open(db.clone()).unwrap();
        {
            let conn = lock(&state.conn);
            // 仓库级状态已迁移
            let st: (String, Option<String>, Option<i64>) = conn
                .query_row(
                    "SELECT last_status, last_message, last_synced FROM repo_state WHERE name = 'alpha'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                st,
                ("success".to_string(), Some("ok".to_string()), Some(1000))
            );
            // 目标级状态已迁移（按目录名关联）
            let t: (String, Option<i64>) = conn
                .query_row(
                    "SELECT last_status, last_synced FROM target_state
                     WHERE repo_name = 'alpha' AND remote = 'gitlab'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(t, ("failed".to_string(), Some(2000)));
            // 队列与停止标记的关联键已换成目录名
            let job: String = conn
                .query_row("SELECT repo_name FROM sync_jobs WHERE id = 1", [], |r| r.get(0))
                .unwrap();
            assert_eq!(job, "alpha", "队列关联键应换成目录名");
            let stop: String = conn
                .query_row("SELECT repo_name FROM sync_stop", [], |r| r.get(0))
                .unwrap();
            assert_eq!(stop, "alpha", "停止标记关联键应换成目录名");
            // 旧表已丢弃
            let old: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'repos'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(old, 0, "旧 repos 表应被丢弃");
        }
        // 幂等：再次打开不报错、不丢数据
        drop(state);
        let state2 = AppState::open(db).unwrap();
        let conn = lock(&state2.conn);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM repo_state WHERE name = 'alpha'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
        drop(conn);
        std::fs::remove_dir_all(&root).ok();
    }
}

/// 命令层测试辅助：tauri::test 的 mock 运行时让 Tauri 命令（State 提取）可脱离窗口直接调用
#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use std::sync::Arc;
    use tauri::Manager;

    /// 独立临时数据库 + 挂载了 AppState 的 mock 应用；root 供调用方清理与构造路径
    pub(crate) fn open_mock_app(
        tag: &str,
    ) -> (tauri::App<tauri::test::MockRuntime>, Arc<AppState>, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("grs-cmd-{tag}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let app = tauri::test::mock_app();
        let state = Arc::new(AppState::open(root.join("app.db")).unwrap());
        app.manage(state.clone());
        (app, state, root)
    }

    /// 直接注入有效会话令牌（登录流程本身由 auth 命令测试覆盖）
    pub(crate) fn insert_session(state: &AppState, token: &str) {
        let conn = lock(&state.conn);
        conn.execute(
            "INSERT INTO sessions (token, username, expires_at) VALUES (?1, 'admin', ?2)",
            params![token, chrono::Utc::now().timestamp() + 3600],
        )
        .unwrap();
    }
}
