//! Model catalog seeding and fallback-chain maintenance.

use rusqlite::Connection;

/// Sentinel for models that have never been ranked by the enrichment service.
pub const UNRANKED_INTELLIGENCE: i64 = 99;
/// Sentinel speed rank for unknown or provider-default speed.
pub const UNRANKED_SPEED: i64 = 10;

/// No hardcoded models are bundled. The models table is populated
/// exclusively by:
///   1. The sync service fetching from the model-discovery crate
///   2. The custom-provider model creation endpoint
///   3. Real-data enrichment updating ranking fields from external APIs
///
/// The schema defaults (intelligence_rank=99, speed_rank=10) mark rows as
/// "unranked" until the enrichment service fetches real benchmark data.
pub fn seed_models(_conn: &Connection) {}

/// Remove orphaned fallback entries, disable
/// fallback entries for disabled models, and inserts a missing fallback entry
/// per model with increasing priority (max priority + 1 .. +n).
pub fn ensure_fallback_entries(conn: &Connection) {
    // 1. Remove orphaned entries (models that no longer exist)
    let orphaned: Vec<i64> = conn
        .prepare(
            "SELECT f.id FROM fallback_config f \
             LEFT JOIN models m ON f.model_db_id = m.id \
             WHERE m.id IS NULL",
        )
        .expect("prepare orphaned fallback select")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("query orphaned fallback ids")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("collect orphaned fallback ids");

    if !orphaned.is_empty() {
        tracing::info!(
            "[seed] Removing {} orphaned fallback entries",
            orphaned.len()
        );
        let placeholders = orphaned.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        conn.execute(
            &format!("DELETE FROM fallback_config WHERE id IN ({placeholders})"),
            rusqlite::params_from_iter(orphaned.iter()),
        )
        .expect("delete orphaned fallback entries");
    }

    // 2. Sync enabled state: disable fallback entries for disabled models
    conn.execute(
        "UPDATE fallback_config SET enabled = 0 \
         WHERE model_db_id IN (SELECT id FROM models WHERE enabled = 0) \
         AND enabled = 1",
        [],
    )
    .expect("disable fallback entries for disabled models");

    // 3. Add missing entries (models without a fallback row), ordered by
    //    intelligence rank and then id.
    let missing: Vec<i64> = conn
        .prepare(
            "SELECT m.id FROM models m \
             LEFT JOIN fallback_config f ON m.id = f.model_db_id \
             WHERE f.id IS NULL \
             ORDER BY m.intelligence_rank ASC, m.id ASC",
        )
        .expect("prepare missing fallback select")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("query missing model ids")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("collect missing model ids");

    if !missing.is_empty() {
        let max_priority: i64 = conn
            .query_row("SELECT MAX(priority) FROM fallback_config", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten()
            .unwrap_or(0);

        for (i, id) in missing.iter().enumerate() {
            conn.execute(
                "INSERT INTO fallback_config (model_db_id, priority, enabled) VALUES (?1, ?2, 1)",
                rusqlite::params![id, max_priority + i as i64 + 1],
            )
            .expect("insert missing fallback entry");
        }
    }
}
