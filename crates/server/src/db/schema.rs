//! Row structs for the SQLite tables defined in `drizzle-orm/schema.ts`
//! (see also the DDL in `connection.rs`). Field names are snake_case and map
//! column-by-column.

use rusqlite::{Row, types::FromSql};

fn opt_text<R: FromSql>(row: &Row<'_>, idx: usize) -> rusqlite::Result<Option<R>> {
    row.get(idx)
}

#[derive(Debug, Clone)]
pub struct ModelRow {
    pub id: i64,
    pub platform: String,
    pub model_id: String,
    pub display_name: String,
    pub intelligence_rank: i64,
    pub speed_rank: i64,
    pub size_label: String,
    pub rpm_limit: Option<i64>,
    pub rpd_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub tpd_limit: Option<i64>,
    pub monthly_token_budget: String,
    pub context_window: Option<i64>,
    pub enabled: i64,
    pub pricing_prompt: Option<f64>,
    pub pricing_completion: Option<f64>,
    pub free_tier: i64,
    pub gateway: Option<String>,
    pub supported_features: Option<String>,
    pub external_url: Option<String>,
    pub description: Option<String>,
    pub last_synced_at: Option<String>,
    pub source: String,
    pub intelligence_score: Option<f64>,
    pub speed_tokens_per_sec: Option<f64>,
    pub ranking_source: Option<String>,
    pub last_ranked_at: Option<String>,
}

pub const MODEL_COLS: &str = "id, platform, model_id, display_name, intelligence_rank, speed_rank, \
     size_label, rpm_limit, rpd_limit, tpm_limit, tpd_limit, monthly_token_budget, context_window, \
     enabled, pricing_prompt, pricing_completion, free_tier, gateway, supported_features, \
     external_url, description, last_synced_at, source, intelligence_score, speed_tokens_per_sec, \
     ranking_source, last_ranked_at";

impl ModelRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            platform: row.get(1)?,
            model_id: row.get(2)?,
            display_name: row.get(3)?,
            intelligence_rank: row.get(4)?,
            speed_rank: row.get(5)?,
            size_label: row.get::<_, String>(6).unwrap_or_default(),
            rpm_limit: opt_text::<i64>(row, 7)?,
            rpd_limit: opt_text::<i64>(row, 8)?,
            tpm_limit: opt_text::<i64>(row, 9)?,
            tpd_limit: opt_text::<i64>(row, 10)?,
            monthly_token_budget: row.get::<_, String>(11).unwrap_or_default(),
            context_window: opt_text::<i64>(row, 12)?,
            enabled: row.get::<_, i64>(13).unwrap_or(1),
            pricing_prompt: opt_text::<f64>(row, 14)?,
            pricing_completion: opt_text::<f64>(row, 15)?,
            free_tier: row.get::<_, i64>(16).unwrap_or(0),
            gateway: opt_text::<String>(row, 17)?,
            supported_features: opt_text::<String>(row, 18)?,
            external_url: opt_text::<String>(row, 19)?,
            description: opt_text::<String>(row, 20)?,
            last_synced_at: opt_text::<String>(row, 21)?,
            source: row.get::<_, String>(22).unwrap_or_else(|_| "manual".into()),
            intelligence_score: opt_text::<f64>(row, 23)?,
            speed_tokens_per_sec: opt_text::<f64>(row, 24)?,
            ranking_source: opt_text::<String>(row, 25)?,
            last_ranked_at: opt_text::<String>(row, 26)?,
        })
    }

    pub fn to_dto(&self) -> crate::types::Model {
        crate::types::Model {
            id: self.id,
            platform: self.platform.clone(),
            model_id: self.model_id.clone(),
            display_name: self.display_name.clone(),
            intelligence_rank: self.intelligence_rank,
            speed_rank: self.speed_rank,
            size_label: self.size_label.clone(),
            rpm_limit: self.rpm_limit,
            rpd_limit: self.rpd_limit,
            tpm_limit: self.tpm_limit,
            tpd_limit: self.tpd_limit,
            monthly_token_budget: self.monthly_token_budget.clone(),
            context_window: self.context_window,
            enabled: self.enabled != 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiKeyRow {
    pub id: i64,
    pub platform: String,
    pub label: String,
    pub encrypted_key: String,
    pub iv: String,
    pub auth_tag: String,
    pub status: String,
    pub enabled: i64,
    pub created_at: String,
    pub last_checked_at: Option<String>,
}

pub const API_KEY_COLS: &str = "id, platform, label, encrypted_key, iv, auth_tag, status, enabled, \
     created_at, last_checked_at";

impl ApiKeyRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            platform: row.get(1)?,
            label: row.get::<_, String>(2).unwrap_or_default(),
            encrypted_key: row.get(3)?,
            iv: row.get(4)?,
            auth_tag: row.get(5)?,
            status: row.get::<_, String>(6).unwrap_or_else(|_| "unknown".into()),
            enabled: row.get::<_, i64>(7).unwrap_or(1),
            created_at: row.get(8)?,
            last_checked_at: opt_text::<String>(row, 9)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RequestRow {
    pub id: i64,
    pub platform: String,
    pub model_id: String,
    pub status: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub error: Option<String>,
    pub created_at: String,
}

impl RequestRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            platform: row.get(1)?,
            model_id: row.get(2)?,
            status: row.get(3)?,
            input_tokens: row.get(4)?,
            output_tokens: row.get(5)?,
            latency_ms: row.get(6)?,
            error: opt_text::<String>(row, 7)?,
            created_at: row.get(8)?,
        })
    }

    pub fn to_dto(&self) -> crate::types::RequestLog {
        crate::types::RequestLog {
            id: self.id,
            platform: self.platform.clone(),
            model_id: self.model_id.clone(),
            status: self.status.clone(),
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            latency_ms: self.latency_ms,
            error: self.error.clone(),
            created_at: self.created_at.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FallbackRow {
    pub id: i64,
    pub model_db_id: i64,
    pub priority: i64,
    pub enabled: i64,
}

impl FallbackRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            model_db_id: row.get(1)?,
            priority: row.get(2)?,
            enabled: row.get::<_, i64>(3).unwrap_or(1),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CustomProviderRow {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub timeout_ms: Option<i64>,
    pub extra_headers: Option<String>,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub const CUSTOM_PROVIDER_COLS: &str =
    "id, name, base_url, timeout_ms, extra_headers, enabled, created_at, updated_at";

impl CustomProviderRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            base_url: row.get(2)?,
            timeout_ms: opt_text::<i64>(row, 3)?,
            extra_headers: opt_text::<String>(row, 4)?,
            enabled: row.get::<_, i64>(5).unwrap_or(1),
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyncLogRow {
    pub id: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub total_discovered: Option<i64>,
    pub added: Option<i64>,
    pub updated: Option<i64>,
    pub disabled: Option<i64>,
    pub free_to_paid: Option<i64>,
    pub paid_to_free: Option<i64>,
    pub stored_disabled: Option<i64>,
    pub error: Option<String>,
}

impl SyncLogRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            started_at: row.get(1)?,
            completed_at: opt_text::<String>(row, 2)?,
            status: row.get::<_, String>(3).unwrap_or_else(|_| "running".into()),
            total_discovered: opt_text::<i64>(row, 4)?,
            added: opt_text::<i64>(row, 5)?,
            updated: opt_text::<i64>(row, 6)?,
            disabled: opt_text::<i64>(row, 7)?,
            free_to_paid: opt_text::<i64>(row, 8)?,
            paid_to_free: opt_text::<i64>(row, 9)?,
            stored_disabled: opt_text::<i64>(row, 10)?,
            error: opt_text::<String>(row, 11)?,
        })
    }
}

/// Helper to read a list of rows for a query string.
pub fn query_rows<T, F>(
    conn: &rusqlite::Connection,
    sql: &str,
    mapper: F,
) -> rusqlite::Result<Vec<T>>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], mapper)?.collect::<rusqlite::Result<Vec<T>>>()?;
    Ok(rows)
}
