//! Ports of `server/src/db/migrations-v1.ts`, `migrations-v4.ts`,
//! `migrations-v5.ts`, `migrations-v12.ts`, `migrations-v13.ts`.
//!
//! Every function mirrors the TS source's literal data values and its
//! idempotency guards (SELECT-then-act, existence checks). UPDATE/DELETE/DDL
//! statements propagate errors via `.expect()` (the TS `.run()` throws on
//! error and aborts the surrounding transaction); SELECTs that may legitimately
//! return no row use `.ok()` to match the TS `.get()`/`.all()` behavior.

use rusqlite::Connection;

use crate::db::seed::{ensure_fallback_entries, UNRANKED_INTELLIGENCE, UNRANKED_SPEED};

/// Mirrors the TS pattern of selecting a model id by (platform, modelId) then
/// deleting its fallback entry and the model row when present.
fn delete_model_if_exists(conn: &Connection, platform: &str, model_id: &str) {
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM models WHERE platform = ?1 AND model_id = ?2",
            rusqlite::params![platform, model_id],
            |row| row.get(0),
        )
        .ok();
    if let Some(id) = id {
        conn.execute(
            "DELETE FROM fallback_config WHERE model_db_id = ?1",
            rusqlite::params![id],
        )
        .expect("delete fallback entry for removed model");
        conn.execute("DELETE FROM models WHERE id = ?1", rusqlite::params![id])
            .expect("delete removed model");
    }
}

/// `PRAGMA table_info(models)` → existing column names (used by v12/v13).
fn existing_columns(conn: &Connection, table: &str) -> Vec<String> {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .expect("prepare PRAGMA table_info")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query PRAGMA table_info")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("collect PRAGMA table_info")
}

/// Whether a table exists in sqlite_master (used by v12).
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
        rusqlite::params![name],
        |row| row.get::<_, String>(0),
    )
    .is_ok()
}

/// `migrateModels(tx)` — migrations-v1.ts.
pub fn migrate_models(conn: &Connection) {
    conn.execute(
        "UPDATE models SET model_id = ?1, display_name = ?2, intelligence_rank = ?3, \
         monthly_token_budget = ?4, rpd_limit = ?5, context_window = ?6, size_label = ?7 \
         WHERE platform = 'openrouter' AND model_id = 'deepseek/deepseek-r1:free'",
        rusqlite::params![
            "deepseek/deepseek-v3.1:free",
            "DeepSeek V3.1 (free)",
            2,
            "~6M",
            200,
            131072,
            "Frontier"
        ],
    )
    .expect("v1: remap openrouter deepseek-r1 to deepseek-v3.1:free");

    conn.execute(
        "UPDATE models SET model_id = ?1, display_name = ?2, intelligence_rank = ?3, \
         monthly_token_budget = ?4, context_window = ?5, size_label = ?6 \
         WHERE platform = 'github' AND model_id = 'gpt-4o'",
        rusqlite::params!["openai/gpt-5", "GPT-5 (GitHub)", 1, "~18M", 128000, "Frontier"],
    )
    .expect("v1: remap github gpt-4o to openai/gpt-5");

    conn.execute(
        "UPDATE models SET rpd_limit = 20, monthly_token_budget = '~3M' \
         WHERE platform = 'google' AND model_id = 'gemini-2.5-flash'",
        [],
    )
    .expect("v1: update google gemini-2.5-flash");
    conn.execute(
        "UPDATE models SET rpm_limit = 20 \
         WHERE platform = 'sambanova' AND model_id = 'Meta-Llama-3.3-70B-Instruct'",
        [],
    )
    .expect("v1: update sambanova Meta-Llama-3.3-70B-Instruct");
    conn.execute(
        "UPDATE models SET tpm_limit = 6000 \
         WHERE platform = 'groq' AND model_id = 'llama-4-scout-17b-16e-instruct'",
        [],
    )
    .expect("v1: update groq llama-4-scout-17b-16e-instruct");
    conn.execute(
        "UPDATE models SET monthly_token_budget = '~1-2M' \
         WHERE platform = 'cohere' AND model_id = 'command-r-plus-08-2024'",
        [],
    )
    .expect("v1: update cohere command-r-plus-08-2024");
    conn.execute(
        "UPDATE models SET monthly_token_budget = '~1-3M' \
         WHERE platform = 'huggingface' AND model_id = 'accounts/fireworks/models/llama-v3p3-70b-instruct'",
        [],
    )
    .expect("v1: update huggingface fireworks llama-v3p3-70b-instruct");
    conn.execute(
        "UPDATE models SET monthly_token_budget = 'credits-based', enabled = 0 \
         WHERE platform = 'nvidia' AND model_id = 'meta/llama-3.1-70b-instruct'",
        [],
    )
    .expect("v1: update nvidia meta/llama-3.1-70b-instruct");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV2(tx)` — migrations-v1.ts.
pub fn migrate_models_v2(conn: &Connection) {
    const REMOVALS: &[(&str, &str)] = &[
        ("cerebras", "qwen-3-coder-480b"),
        ("cerebras", "llama-4-maverick-17b-128e-instruct"),
        ("cerebras", "gpt-oss-120b"),
        ("openrouter", "deepseek/deepseek-v3.1:free"),
        ("openrouter", "moonshotai/kimi-k2:free"),
    ];
    for &(platform, model_id) in REMOVALS {
        delete_model_if_exists(conn, platform, model_id);
    }

    conn.execute(
        "UPDATE models SET model_id = ?1, display_name = ?2, intelligence_rank = ?3, \
         size_label = ?4, context_window = ?5, monthly_token_budget = ?6 \
         WHERE platform = 'github' AND model_id = 'openai/gpt-5'",
        rusqlite::params!["gpt-4o", "GPT-4o", 5, "Large", 8000, "~18M"],
    )
    .expect("v2: remap github openai/gpt-5 back to gpt-4o");

    conn.execute(
        "UPDATE models SET model_id = 'meta-llama/llama-4-scout-17b-16e-instruct' \
         WHERE platform = 'groq' AND model_id = 'llama-4-scout-17b-16e-instruct'",
        [],
    )
    .expect("v2: rename groq llama-4-scout-17b-16e-instruct");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV3Ranks(tx)` — migrations-v4.ts. No-op: hardcoded rank
/// UPDATEs were removed (ranks come from the enrichment service).
pub fn migrate_models_v3_ranks(_conn: &Connection) {}

/// `migrateModelsV4(tx)` — migrations-v4.ts.
pub fn migrate_models_v4(conn: &Connection) {
    const REMOVALS: &[(&str, &str)] = &[
        ("moonshot", "kimi-latest"),
        ("minimax", "MiniMax-M1"),
        ("openrouter", "google/gemma-4-31b-it:free"),
        ("huggingface", "accounts/fireworks/models/llama-v3p3-70b-instruct"),
    ];
    for &(platform, model_id) in REMOVALS {
        delete_model_if_exists(conn, platform, model_id);
    }

    conn.execute(
        "UPDATE models SET model_id = ?1, display_name = ?2, context_window = ?3 \
         WHERE platform = 'cloudflare' AND model_id = '@cf/meta/llama-3.1-70b-instruct'",
        rusqlite::params![
            "@cf/meta/llama-3.3-70b-instruct-fp8-fast",
            "Llama 3.3 70B fp8-fast (CF)",
            131072
        ],
    )
    .expect("v4: remap cloudflare llama-3.1 to llama-3.3 fp8-fast");
    conn.execute(
        "UPDATE models SET tpm_limit = 12000 \
         WHERE platform = 'groq' AND model_id = 'llama-3.3-70b-versatile'",
        [],
    )
    .expect("v4: update groq llama-3.3-70b-versatile");
    conn.execute(
        "UPDATE models SET rpd_limit = 20 \
         WHERE platform = 'sambanova' AND model_id = 'Meta-Llama-3.3-70B-Instruct'",
        [],
    )
    .expect("v4: update sambanova Meta-Llama-3.3-70B-Instruct");
    conn.execute(
        "UPDATE models SET rpd_limit = 14400 \
         WHERE platform = 'cerebras' AND model_id = 'qwen-3-235b-a22b-instruct-2507'",
        [],
    )
    .expect("v4: update cerebras qwen-3-235b-a22b-instruct-2507");
    conn.execute(
        "UPDATE models SET rpd_limit = 250, monthly_token_budget = '~25M' \
         WHERE platform = 'google' AND model_id = 'gemini-2.5-flash'",
        [],
    )
    .expect("v4: update google gemini-2.5-flash");
    conn.execute(
        "UPDATE models SET rpd_limit = 50, monthly_token_budget = '~6M' \
         WHERE platform = 'google' AND model_id = 'gemini-2.5-pro'",
        [],
    )
    .expect("v4: update google gemini-2.5-pro");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV5(tx)` — migrations-v5.ts.
pub fn migrate_models_v5(conn: &Connection) {
    conn.execute(
        "UPDATE models SET enabled = 0 \
         WHERE platform = 'google' AND model_id = 'gemini-2.5-pro'",
        [],
    )
    .expect("v5: disable google gemini-2.5-pro");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV6(tx)` — migrations-v5.ts.
pub fn migrate_models_v6(conn: &Connection) {
    delete_model_if_exists(conn, "openrouter", "arcee-ai/trinity-large-preview:free");

    conn.execute(
        "UPDATE models SET rpd_limit = 20, monthly_token_budget = '~3M' \
         WHERE platform = 'google' AND model_id = 'gemini-2.5-flash'",
        [],
    )
    .expect("v6: update google gemini-2.5-flash");
    conn.execute(
        "UPDATE models SET rpd_limit = 20, monthly_token_budget = '~3M' \
         WHERE platform = 'google' AND model_id = 'gemini-2.5-flash-lite'",
        [],
    )
    .expect("v6: update google gemini-2.5-flash-lite");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV7(tx)` — migrations-v5.ts.
pub fn migrate_models_v7(conn: &Connection) {
    delete_model_if_exists(conn, "openrouter", "inclusionai/ling-2.6-flash:free");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV8(tx)` — migrations-v5.ts.
pub fn migrate_models_v8(conn: &Connection) {
    ensure_fallback_entries(conn);
}

/// `migrateModelsV9(tx)` — migrations-v5.ts. Note: TS does NOT call
/// ensureFallbackEntries here, so neither do we.
pub fn migrate_models_v9(conn: &Connection) {
    conn.execute(
        "UPDATE models SET enabled = 0 \
         WHERE platform = 'cerebras' AND model_id = 'zai-glm-4.7'",
        [],
    )
    .expect("v9: disable cerebras zai-glm-4.7");
}

/// `migrateModelsV10(tx)` — migrations-v5.ts.
pub fn migrate_models_v10(conn: &Connection) {
    ensure_fallback_entries(conn);
}

/// `migrateModelsV11(tx)` — migrations-v5.ts.
pub fn migrate_models_v11(conn: &Connection) {
    conn.execute(
        "UPDATE models SET model_id = 'qwen-3-235b-a22b-instruct-2507' \
         WHERE platform = 'cerebras' AND model_id = 'qwen3-235b'",
        [],
    )
    .expect("v11: rename cerebras qwen3-235b");
    conn.execute(
        "UPDATE models SET enabled = 1, monthly_token_budget = '~3M (1k credits)' \
         WHERE platform = 'nvidia' AND model_id = 'meta/llama-3.1-70b-instruct'",
        [],
    )
    .expect("v11: re-enable nvidia meta/llama-3.1-70b-instruct");

    ensure_fallback_entries(conn);
}

/// `migrateModelsV12(tx)` — migrations-v12.ts. Idempotent column backfill for
/// the sync-related columns/tables (create_tables has already applied them on
/// fresh DBs, so the existence checks are no-ops there).
pub fn migrate_models_v12(conn: &Connection) {
    const MODEL_NEW_COLS: &[(&str, &str)] = &[
        ("pricing_prompt", "REAL"),
        ("pricing_completion", "REAL"),
        ("free_tier", "INTEGER NOT NULL DEFAULT 0"),
        ("gateway", "TEXT"),
        ("supported_features", "TEXT"),
        ("external_url", "TEXT"),
        ("description", "TEXT"),
        ("last_synced_at", "TEXT"),
        ("source", "TEXT NOT NULL DEFAULT 'manual'"),
    ];
    let existing = existing_columns(conn, "models");
    for &(col_name, col_def) in MODEL_NEW_COLS {
        if !existing.iter().any(|c| c == col_name) {
            conn.execute_batch(&format!("ALTER TABLE models ADD COLUMN {col_name} {col_def}"))
                .expect("v12: add models column");
        }
    }

    if !table_exists(conn, "sync_log") {
        conn.execute_batch(
            "CREATE TABLE sync_log (
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
            )",
        )
        .expect("v12: create sync_log");
    }

    if !table_exists(conn, "sync_changes") {
        conn.execute_batch(
            "CREATE TABLE sync_changes (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              sync_log_id INTEGER NOT NULL REFERENCES sync_log(id),
              change_type TEXT NOT NULL,
              platform TEXT NOT NULL,
              model_id TEXT NOT NULL,
              display_name TEXT,
              details TEXT,
              created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .expect("v12: create sync_changes");
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_sync_changes_log_id ON sync_changes(sync_log_id)",
        )
        .expect("v12: create idx_sync_changes_log_id");
    }

    if !table_exists(conn, "custom_providers") {
        conn.execute_batch(
            "CREATE TABLE custom_providers (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL UNIQUE,
              base_url TEXT NOT NULL,
              timeout_ms INTEGER DEFAULT 15000,
              extra_headers TEXT,
              enabled INTEGER NOT NULL DEFAULT 1,
              created_at TEXT NOT NULL DEFAULT (datetime('now')),
              updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .expect("v12: create custom_providers");
    }
}

/// `migrateModelsV13(tx)` — migrations-v13.ts. Resets every row's
/// intelligence/speed rank back to the unranked sentinel and re-sequences
/// fallback priorities (unranked rows sort last but stay enabled).
pub fn migrate_models_v13(conn: &Connection) {
    const RANKING_COLS: &[(&str, &str)] = &[
        ("intelligence_score", "REAL"),
        ("speed_tokens_per_sec", "REAL"),
        ("ranking_source", "TEXT"),
        ("last_ranked_at", "TEXT"),
    ];
    let existing = existing_columns(conn, "models");
    for &(col_name, col_def) in RANKING_COLS {
        if !existing.iter().any(|c| c == col_name) {
            conn.execute_batch(&format!("ALTER TABLE models ADD COLUMN {col_name} {col_def}"))
                .expect("v13: add ranking column");
        }
    }

    conn.execute(
        "UPDATE models SET intelligence_rank = ?1, speed_rank = ?2, \
         intelligence_score = NULL, speed_tokens_per_sec = NULL, \
         ranking_source = NULL, last_ranked_at = NULL",
        rusqlite::params![UNRANKED_INTELLIGENCE, UNRANKED_SPEED],
    )
    .expect("v13: reset ranking columns on all models");

    let ordered: Vec<i64> = conn
        .prepare(
            "SELECT id FROM models \
             ORDER BY CASE WHEN intelligence_rank = ?1 THEN 1 ELSE 0 END, intelligence_rank, id",
        )
        .expect("v13: prepare ordered models select")
        .query_map(rusqlite::params![UNRANKED_INTELLIGENCE], |row| row.get::<_, i64>(0))
        .expect("v13: query ordered models")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("v13: collect ordered models");

    conn.execute("DELETE FROM fallback_config", [])
        .expect("v13: clear fallback_config");
    for (i, id) in ordered.iter().enumerate() {
        conn.execute(
            "INSERT INTO fallback_config (model_db_id, priority, enabled) VALUES (?1, ?2, 1)",
            rusqlite::params![id, i as i64 + 1],
        )
        .expect("v13: insert re-sequenced fallback entry");
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn run_migrations_is_idempotent() {
        std::env::set_var("ENCRYPTION_KEY", "a".repeat(64));
        crate::db::init_db(Some(":memory:")).unwrap();

        let conn = crate::db::db().blocking_lock();
        let conn: &Connection = &conn;

        // The migrations are pure UPDATE/DELETE/DDL (seed_models is a no-op),
        // so a fresh DB starts with zero models. Insert a small fixture first —
        // mirroring the TS tests' runMigrations-then-seedTestModels setup —
        // so the v1..v13 statements have rows to act on.
        let fixture = [
            ("github", "gpt-4o", "GPT-4o", 5_i64, 4_i64),
            ("google", "gemini-2.5-flash", "Gemini 2.5 Flash", 3_i64, 2_i64),
            ("openrouter", "deepseek/deepseek-r1:free", "DeepSeek R1 (free)", 2_i64, 1_i64),
            ("unknown", "not-yet-ranked", "Not Yet Ranked", 99_i64, 10_i64),
        ];
        for (platform, model_id, display_name, int_rank, spd_rank) in fixture {
            conn.execute(
                "INSERT INTO models (platform, model_id, display_name, intelligence_rank, speed_rank) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![platform, model_id, display_name, int_rank, spd_rank],
            )
            .expect("insert fixture model");
        }

        crate::db::run_migrations(conn);

        let model_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0))
            .unwrap();
        let fallback_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM fallback_config", [], |r| r.get(0))
            .unwrap();
        let key_value: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'unified_api_key'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert!(model_count > 0, "expected fixture models to survive migrations");
        assert!(
            !key_value.is_empty(),
            "unified_api_key setting should exist after run_migrations"
        );
        assert_eq!(
            fallback_count, model_count,
            "every model should have exactly one fallback entry"
        );

        // Second run must not change any row counts (idempotency).
        crate::db::run_migrations(conn);

        let model_count_2: i64 = conn
            .query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0))
            .unwrap();
        let fallback_count_2: i64 = conn
            .query_row("SELECT COUNT(*) FROM fallback_config", [], |r| r.get(0))
            .unwrap();
        let key_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'unified_api_key'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(model_count, model_count_2, "models row count must be stable");
        assert_eq!(
            fallback_count, fallback_count_2,
            "fallback_config row count must be stable"
        );
        assert_eq!(key_count, 1, "unified_api_key must still exist exactly once");
    }
}
