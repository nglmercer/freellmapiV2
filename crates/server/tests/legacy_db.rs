//! Compatibility coverage for databases created by the former server.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

const LEGACY_KEY: &str = "45fe82ca573343475996890256d5220bcff28c";
const LEGACY_IV: &str = "00112233445566778899aabbccddeeff";
const LEGACY_TAG: &str = "fd1669cf1d462b755ca503cd74347d3c";

fn temporary_db_path() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "freellmapi-legacy-db-{}-{suffix}.db",
        std::process::id()
    ))
}

fn create_legacy_database(path: &PathBuf) {
    let conn = Connection::open(path).expect("open legacy database");
    conn.execute_batch(
        "
        CREATE TABLE models (
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
          UNIQUE(platform, model_id)
        );

        CREATE TABLE api_keys (
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

        CREATE TABLE requests (
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

        CREATE TABLE fallback_config (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          model_db_id INTEGER NOT NULL REFERENCES models(id),
          priority INTEGER NOT NULL,
          enabled INTEGER NOT NULL DEFAULT 1,
          UNIQUE(model_db_id)
        );

        CREATE TABLE settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE TABLE custom_providers (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          name TEXT NOT NULL UNIQUE,
          base_url TEXT NOT NULL,
          timeout_ms INTEGER DEFAULT 15000,
          extra_headers TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )
    .expect("create legacy schema");

    conn.execute(
        "INSERT INTO models
         (platform, model_id, display_name, intelligence_rank, speed_rank,
          size_label, context_window)
         VALUES ('legacy', 'legacy-first', 'Legacy First', 4, 2, 'Small', 8192)",
        [],
    )
    .expect("insert first legacy model");
    let first_id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO models
         (platform, model_id, display_name, intelligence_rank, speed_rank,
          size_label, context_window)
         VALUES ('legacy', 'legacy-second', 'Legacy Second', 2, 1, 'Large', 32768)",
        [],
    )
    .expect("insert second legacy model");
    let second_id = conn.last_insert_rowid();

    // The second model was deliberately placed first by the user. Migration
    // must retain this order even though it refreshes ranking metadata.
    conn.execute(
        "INSERT INTO fallback_config (model_db_id, priority, enabled)
         VALUES (?1, 8, 1), (?2, 2, 1)",
        rusqlite::params![first_id, second_id],
    )
    .expect("insert legacy fallback configuration");

    conn.execute(
        "INSERT INTO api_keys
         (platform, label, encrypted_key, iv, auth_tag, status, enabled)
         VALUES ('legacy', 'old key', ?1, ?2, ?3, 'healthy', 1)",
        rusqlite::params![LEGACY_KEY, LEGACY_IV, LEGACY_TAG],
    )
    .expect("insert legacy encrypted key");
    conn.execute(
        "INSERT INTO settings (key, value) VALUES
         ('unified_api_key', 'legacy-unified-key'),
         ('dashboard_theme', 'dark')",
        [],
    )
    .expect("insert legacy settings");
    conn.execute(
        "INSERT INTO custom_providers
         (name, base_url, timeout_ms, extra_headers, enabled)
         VALUES ('Legacy Gateway', 'https://legacy.example/v1', 21000,
                 '{\"X-Legacy\":\"yes\"}', 1)",
        [],
    )
    .expect("insert legacy custom provider");
    conn.execute(
        "INSERT INTO requests
         (platform, model_id, status, input_tokens, output_tokens, latency_ms, error)
         VALUES ('legacy', 'legacy-second', 'success', 3, 5, 120, NULL)",
        [],
    )
    .expect("insert legacy request history");
}

async fn fallback_snapshot() -> Vec<(String, i64, i64)> {
    let conn = server::db::db().lock().await;
    let mut statement = conn
        .prepare(
            "SELECT m.model_id, f.priority, f.enabled
             FROM fallback_config AS f
             JOIN models AS m ON m.id = f.model_db_id
             ORDER BY f.priority ASC",
        )
        .expect("prepare fallback snapshot");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query fallback snapshot")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect fallback snapshot")
}

#[tokio::test]
async fn migrates_realistic_legacy_database_without_losing_data() {
    std::env::set_var("ENCRYPTION_KEY", "aa".repeat(32));
    let path = temporary_db_path();
    create_legacy_database(&path);

    server::db::init_db(path.to_str()).expect("initialize legacy database");
    server::db::run_in_transaction(|conn| {
        server::db::run_migrations(conn);
        Ok::<(), rusqlite::Error>(())
    })
    .await
    .expect("migrate legacy database");

    let first_fallback_snapshot = fallback_snapshot().await;
    assert_eq!(
        first_fallback_snapshot,
        vec![
            ("legacy-second".to_string(), 1, 1),
            ("legacy-first".to_string(), 2, 1),
        ],
        "the user's fallback order must survive migration"
    );

    {
        let conn = server::db::db().lock().await;
        let model_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM models", [], |row| row.get(0))
            .unwrap();
        assert_eq!(model_count, 2);

        let key: (String, String, String) = conn
            .query_row(
                "SELECT encrypted_key, iv, auth_tag FROM api_keys WHERE platform = 'legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            server::crypto::decrypt(&key.0, &key.1, &key.2).unwrap(),
            "legacy-provider-key"
        );

        let unified: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'unified_api_key'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unified, "legacy-unified-key");
        let theme: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'dashboard_theme'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(theme, "dark");

        let custom_provider: (String, i64, String) = conn
            .query_row(
                "SELECT base_url, timeout_ms, extra_headers
                 FROM custom_providers WHERE name = 'Legacy Gateway'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(custom_provider.0, "https://legacy.example/v1");
        assert_eq!(custom_provider.1, 21000);
        assert_eq!(custom_provider.2, r#"{"X-Legacy":"yes"}"#);

        let request_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))
            .unwrap();
        assert_eq!(request_count, 1);

        for column in [
            "pricing_prompt",
            "free_tier",
            "source",
            "intelligence_score",
            "speed_tokens_per_sec",
            "ranking_source",
            "last_ranked_at",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('models') WHERE name = ?1",
                    rusqlite::params![column],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "missing migrated models column {column}");
        }

        let sync_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('sync_log', 'sync_changes')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sync_tables, 2);

        conn.execute(
            "INSERT INTO sync_log (started_at, completed_at, status, total_discovered)
             VALUES ('2026-01-01T00:00:00Z', '2026-01-01T00:00:01Z', 'completed', 2)",
            [],
        )
        .unwrap();
        let sync_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO sync_changes
             (sync_log_id, change_type, platform, model_id, display_name, details)
             VALUES (?1, 'updated', 'legacy', 'legacy-second', 'Legacy Second', '{}')",
            rusqlite::params![sync_id],
        )
        .unwrap();
    }

    // A second startup migration must leave all preserved data and migration
    // tables usable and must not generate a replacement unified key.
    server::db::run_in_transaction(|conn| {
        server::db::run_migrations(conn);
        Ok::<(), rusqlite::Error>(())
    })
    .await
    .expect("repeat legacy migration");

    assert_eq!(fallback_snapshot().await[0].0, "legacy-second");
    {
        let conn = server::db::db().lock().await;
        let unified: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'unified_api_key'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unified, "legacy-unified-key");
        let sync_counts: (i64, i64) = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM sync_log),
                        (SELECT COUNT(*) FROM sync_changes)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(sync_counts, (1, 1));
        let request_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))
            .unwrap();
        assert_eq!(request_count, 1);
    }

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(path.with_extension("db-wal")).ok();
    std::fs::remove_file(path.with_extension("db-shm")).ok();
}
