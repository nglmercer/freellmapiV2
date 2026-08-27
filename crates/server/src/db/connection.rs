//! SQLite connection management for the server.
//!
//! The server intentionally uses one synchronous `rusqlite::Connection`
//! behind a `tokio::sync::Mutex`. Guards are held only across synchronous
//! database work, and transactions commit or roll back as a unit.

use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;

use rusqlite::Connection;
use tokio::sync::Mutex;

pub static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

/// Overridable at runtime by tests before first initialization.
static DB_PATH_OVERRIDE: StdMutex<Option<String>> = StdMutex::new(None);

fn default_db_path(root: &Path) -> PathBuf {
    let legacy = root.join("server/data/freeapi.db");
    if legacy.is_file() {
        legacy
    } else {
        root.join("data/freeapi.db")
    }
}

pub fn db_path() -> String {
    if let Some(p) = crate::env::env_string("DB_PATH") {
        return p;
    }
    let override_path = DB_PATH_OVERRIDE.lock().unwrap().clone();
    override_path.unwrap_or_else(|| {
        default_db_path(&crate::env::project_root())
            .to_string_lossy()
            .into_owned()
    })
}

pub fn set_db_path_override(path: &str) {
    *DB_PATH_OVERRIDE.lock().unwrap() = Some(path.to_string());
}

/// Opens the database, applies pragmas, creates tables, initializes the
/// encryption key, and installs the global connection.
pub fn init_db(path: Option<&str>) -> rusqlite::Result<()> {
    let resolved = path.map(|s| s.to_string()).unwrap_or_else(db_path);
    let is_memory = resolved == ":memory:";

    if !is_memory {
        if let Some(parent) = Path::new(&resolved).parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    let sqlite = if is_memory {
        Connection::open_in_memory()?
    } else {
        Connection::open(&resolved)?
    };
    if !is_memory {
        sqlite.execute_batch("PRAGMA journal_mode = WAL").ok();
    }
    sqlite.execute_batch("PRAGMA foreign_keys = ON")?;

    create_tables(&sqlite)?;
    crate::crypto::init_encryption_key(&sqlite);

    // OnceLock::set is once-only; tests re-run init_db, so swap the inner
    // connection when the global already exists. try_lock (not blocking_lock)
    // so this stays callable from async test runtimes.
    match DB.get() {
        Some(m) => match m.try_lock() {
            Ok(mut g) => *g = sqlite,
            Err(_) => {
                return Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                    Some("database connection is in use; cannot re-init".to_string()),
                ))
            }
        },
        None => {
            let _ = DB.set(Mutex::new(sqlite));
        }
    }
    Ok(())
}

/// Returns the shared connection lock. Provider HTTP calls must happen after
/// the database guard is dropped.
pub fn db() -> &'static Mutex<Connection> {
    DB.get()
        .expect("Database not initialized. Call init_db() first.")
}

/// Runs `f` inside a transaction. Rollback happens on `Err` when the
/// transaction is dropped.
pub async fn run_in_transaction<T, F>(f: F) -> rusqlite::Result<T>
where
    F: FnOnce(&Connection) -> rusqlite::Result<T>,
{
    let mut conn = db().lock().await;
    let tx = conn.transaction()?;
    let result = f(&tx)?;
    tx.commit()?;
    Ok(result)
}

/// Idempotent table creation and column backfills for current and legacy DBs.
fn create_tables(sqlite: &Connection) -> rusqlite::Result<()> {
    sqlite.execute_batch(
        "
    CREATE TABLE IF NOT EXISTS models (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      display_name TEXT NOT NULL,
      intelligence_rank INTEGER NOT NULL,
      speed_rank INTEGER NOT NULL,
      size_label TEXT NOT NULL DEFAULT '',
      rpm_limit INTEGER,
      rpd_limit INTEGER,
      tpm_limit INTEGER,
      tpd_limit INTEGER,
      monthly_token_budget TEXT NOT NULL DEFAULT '',
      context_window INTEGER,
      enabled INTEGER NOT NULL DEFAULT 1,
      pricing_prompt REAL,
      pricing_completion REAL,
      free_tier INTEGER NOT NULL DEFAULT 0,
      gateway TEXT,
      supported_features TEXT,
      external_url TEXT,
      description TEXT,
      last_synced_at TEXT,
      source TEXT NOT NULL DEFAULT 'manual',
      intelligence_score REAL,
      speed_tokens_per_sec REAL,
      ranking_source TEXT,
      last_ranked_at TEXT,
      UNIQUE(platform, model_id)
    );

    CREATE TABLE IF NOT EXISTS sync_log (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      started_at TEXT NOT NULL,
      completed_at TEXT,
      status TEXT NOT NULL DEFAULT 'running',
      total_discovered INTEGER DEFAULT 0,
      added INTEGER DEFAULT 0,
      updated INTEGER DEFAULT 0,
      disabled INTEGER DEFAULT 0,
      free_to_paid INTEGER DEFAULT 0,
      paid_to_free INTEGER DEFAULT 0,
      stored_disabled INTEGER DEFAULT 0,
      error TEXT
    );

    CREATE TABLE IF NOT EXISTS sync_changes (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      sync_log_id INTEGER NOT NULL REFERENCES sync_log(id),
      change_type TEXT NOT NULL,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      display_name TEXT,
      details TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS api_keys (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      platform TEXT NOT NULL,
      label TEXT NOT NULL DEFAULT '',
      encrypted_key TEXT NOT NULL,
      iv TEXT NOT NULL,
      auth_tag TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'unknown',
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      last_checked_at TEXT
    );

    CREATE TABLE IF NOT EXISTS requests (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      status TEXT NOT NULL,
      input_tokens INTEGER NOT NULL DEFAULT 0,
      output_tokens INTEGER NOT NULL DEFAULT 0,
      latency_ms INTEGER NOT NULL DEFAULT 0,
      error TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS fallback_config (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      model_db_id INTEGER NOT NULL REFERENCES models(id),
      priority INTEGER NOT NULL,
      enabled INTEGER NOT NULL DEFAULT 1,
      UNIQUE(model_db_id)
    );

    CREATE TABLE IF NOT EXISTS settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS custom_providers (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL UNIQUE,
      base_url TEXT NOT NULL,
      timeout_ms INTEGER DEFAULT 15000,
      extra_headers TEXT,
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE INDEX IF NOT EXISTS idx_requests_created_at ON requests(created_at);
    CREATE INDEX IF NOT EXISTS idx_requests_platform ON requests(platform);
    CREATE INDEX IF NOT EXISTS idx_api_keys_platform ON api_keys(platform);
    CREATE INDEX IF NOT EXISTS idx_sync_changes_log_id ON sync_changes(sync_log_id);
    ",
    )?;

    // Add columns that may not exist on pre-existing databases (idempotent)
    let new_cols: &[(&str, &str)] = &[
        ("pricing_prompt", "REAL"),
        ("pricing_completion", "REAL"),
        ("free_tier", "INTEGER NOT NULL DEFAULT 0"),
        ("gateway", "TEXT"),
        ("supported_features", "TEXT"),
        ("external_url", "TEXT"),
        ("description", "TEXT"),
        ("last_synced_at", "TEXT"),
        ("source", "TEXT NOT NULL DEFAULT 'manual'"),
        ("intelligence_score", "REAL"),
        ("speed_tokens_per_sec", "REAL"),
        ("ranking_source", "TEXT"),
        ("last_ranked_at", "TEXT"),
    ];
    let existing: Vec<String> = existing_columns(sqlite, "models")?;
    for (name, def) in new_cols {
        if !existing.iter().any(|c| c == name) {
            sqlite.execute_batch(&format!("ALTER TABLE models ADD COLUMN {name} {def}"))?;
        }
    }

    // Index is created after the column-add loop so the column is guaranteed
    // to exist (CREATE INDEX in the same exec batch fails on older DBs where
    // the column hadn't been added yet).
    let existing_after: Vec<String> = existing_columns(sqlite, "models")?;
    if existing_after.iter().any(|c| c == "last_ranked_at") {
        sqlite.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_models_last_ranked_at ON models(last_ranked_at)",
        )?;
    }
    Ok(())
}

fn existing_columns(sqlite: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = sqlite.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "freellmapi-db-path-{}-{suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn new_default_uses_data_directory() {
        let root = unique_root();
        assert_eq!(default_db_path(&root), root.join("data/freeapi.db"));
    }

    #[test]
    fn existing_legacy_database_wins() {
        let root = unique_root();
        let legacy = root.join("server/data/freeapi.db");
        std::fs::create_dir_all(legacy.parent().expect("legacy path has a parent")).unwrap();
        std::fs::File::create(&legacy).unwrap();

        assert_eq!(default_db_path(&root), legacy);

        std::fs::remove_dir_all(root).unwrap();
    }
}
