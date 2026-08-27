//! Port of `server/src/providers/custom.ts`.

use std::collections::HashMap;
use std::sync::Arc;

use rusqlite::Connection;

use super::base::Provider;
use super::openai_compat::{OpenAICompatOptions, OpenAICompatProvider};
use crate::db::schema::{CustomProviderRow, CUSTOM_PROVIDER_COLS};

const CUSTOM_PREFIX: &str = "custom:";

pub fn provider_id_to_platform(id: i64) -> String {
    format!("{CUSTOM_PREFIX}{id}")
}

pub fn platform_to_provider_id(platform: &str) -> Option<i64> {
    platform
        .strip_prefix(CUSTOM_PREFIX)
        .and_then(|s| s.parse::<i64>().ok())
}

pub fn has_custom_provider(platform: &str) -> bool {
    platform.starts_with(CUSTOM_PREFIX)
}

fn build_provider(row: &CustomProviderRow, platform: &str) -> Arc<dyn Provider> {
    let mut extra_headers: Vec<(String, String)> = Vec::new();
    if let Some(raw) = &row.extra_headers {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw)
        {
            for (k, v) in parsed {
                if let Some(s) = v.as_str() {
                    extra_headers.push((k, s.to_string()));
                }
            }
        }
    }

    Arc::new(OpenAICompatProvider::new(OpenAICompatOptions {
        platform: platform.to_string(),
        name: row.name.clone(),
        base_url: row.base_url.clone(),
        extra_headers,
        validate_url: None,
        timeout_ms: row.timeout_ms.unwrap_or(15000) as u64,
    }))
}

pub fn load_custom_provider(conn: &Connection, platform: &str) -> Option<Arc<dyn Provider>> {
    let provider_id = platform_to_provider_id(platform)?;
    let sql = format!(
        "SELECT {CUSTOM_PROVIDER_COLS} FROM custom_providers WHERE id = ?1 AND enabled = 1"
    );
    let row = conn
        .query_row(&sql, rusqlite::params![provider_id], CustomProviderRow::from_row)
        .ok()?;
    Some(build_provider(&row, platform))
}

pub fn load_all_custom_providers(conn: &Connection) -> HashMap<String, Arc<dyn Provider>> {
    let mut map = HashMap::new();
    let sql = format!(
        "SELECT {CUSTOM_PROVIDER_COLS} FROM custom_providers WHERE enabled = 1"
    );
    let rows = conn
        .prepare(&sql)
        .and_then(|mut stmt| {
            stmt.query_map([], CustomProviderRow::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    for row in rows {
        let platform = provider_id_to_platform(row.id);
        map.insert(platform.clone(), build_provider(&row, &platform));
    }
    map
}
