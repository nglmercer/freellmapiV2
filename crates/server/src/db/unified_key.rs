//! Unified proxy API-key persistence.

use rusqlite::Connection;

use crate::db::connection::db;

fn random_key_hex() -> String {
    let mut bytes = [0u8; 24];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    hex::encode(bytes)
}

pub fn ensure_unified_key(conn: &Connection) {
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'unified_api_key'",
            [],
            |row| row.get(0),
        )
        .ok();
    if existing.is_none() {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('unified_api_key', ?1)",
            rusqlite::params![random_key_hex()],
        )
        .ok();
    }
}

pub async fn get_unified_api_key() -> String {
    let conn = db().lock().await;
    let row: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'unified_api_key'",
            [],
            |r| r.get(0),
        )
        .ok();
    match row {
        Some(v) => v,
        None => {
            let default_key = random_key_hex();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('unified_api_key', ?1)",
                rusqlite::params![default_key],
            )
            .ok();
            default_key
        }
    }
}

pub async fn regenerate_unified_key() -> String {
    let key = random_key_hex();
    let conn = db().lock().await;
    conn.execute(
        "UPDATE settings SET value = ?1 WHERE key = 'unified_api_key'",
        rusqlite::params![key],
    )
    .ok();
    key
}
