//! Model discovery synchronization and change logging.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::Serialize;

use crate::db::connection::db;
use crate::db::schema::{ModelRow, MODEL_COLS};
use crate::services::model_sync::mappings::{get_platform_by_provider, CURATION_DEFAULTS};
use crate::services::rankings::enrich::enrich_rankings;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncChange {
    #[serde(rename = "changeType")]
    pub change_type: String, // 'added' | 'updated' | 'disabled' | 'free_to_paid' | 'paid_to_free' | 'stored'
    pub platform: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnrichSummary {
    pub scanned: i64,
    pub updated: i64,
    pub skipped: i64,
    pub source: String,
    pub duration_ms: i64,
    pub errors: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub total_discovered: i64,
    pub added: i64,
    pub updated: i64,
    pub disabled: i64,
    pub free_to_paid: i64,
    pub paid_to_free: i64,
    pub stored_disabled: i64,
    pub changes: Vec<SyncChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrich: Option<EnrichSummary>,
}

struct InsertPlan {
    platform: String,
    model_id: String,
    display_name: String,
    context_window: Option<i64>,
    pricing_prompt: Option<f64>,
    pricing_completion: Option<f64>,
    free_tier: i64,
    gateway: Option<String>,
    supported_features: String,
    external_url: Option<String>,
    description: Option<String>,
    last_synced_at: String,
    enabled: i64,
}

struct UpdatePlan {
    id: i64,
    display_name: String,
    context_window: Option<i64>,
    pricing_prompt: Option<f64>,
    pricing_completion: Option<f64>,
    free_tier: i64,
    gateway: Option<String>,
    supported_features: String,
    external_url: Option<String>,
    description: Option<String>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub async fn sync_models() -> Result<SyncResult, String> {
    let started_at = now_iso();

    let log_id = {
        let conn = db().lock().await;
        conn.execute(
            "INSERT INTO sync_log (started_at, status) VALUES (?1, 'running')",
            rusqlite::params![started_at],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };

    match sync_models_inner(&started_at, log_id).await {
        Ok(result) => Ok(result),
        Err(err) => {
            let conn = db().lock().await;
            conn.execute(
                "UPDATE sync_log SET completed_at = ?1, status = 'failed', error = ?2 WHERE id = ?3",
                rusqlite::params![now_iso(), err, log_id],
            )
            .ok();
            Err(err)
        }
    }
}

async fn sync_models_inner(started_at: &str, log_id: i64) -> Result<SyncResult, String> {
    let external_models = getmodelsapi::get_models(getmodelsapi::GetModelsOptions {
        limit: Some(2000),
        ..Default::default()
    })
    .await;

    let mut result = SyncResult {
        total_discovered: external_models.len() as i64,
        added: 0,
        updated: 0,
        disabled: 0,
        free_to_paid: 0,
        paid_to_free: 0,
        stored_disabled: 0,
        changes: Vec::new(),
        enrich: None,
    };

    let mut conn = db().lock().await;

    let current_models: Vec<ModelRow> = {
        let sql = format!("SELECT {MODEL_COLS} FROM models");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], ModelRow::from_row)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect::<Vec<ModelRow>>();
        drop(stmt);
        rows
    };
    let mut current_map: HashMap<String, ModelRow> = HashMap::new();
    for m in current_models {
        current_map.insert(format!("{}:{}", m.platform, m.model_id), m);
    }

    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut new_model_inserts: Vec<InsertPlan> = Vec::new();
    let mut updates: Vec<UpdatePlan> = Vec::new();
    let mut disables: Vec<i64> = Vec::new();

    for ext in &external_models {
        let Some(platform) = get_platform_by_provider(&ext.provider) else {
            continue;
        };

        let key = format!("{platform}:{}", ext.id);
        seen_keys.insert(key.clone());
        let existing = current_map.get(&key);
        let is_free = ext.free_tier == Some(true);

        match existing {
            None => {
                new_model_inserts.push(InsertPlan {
                    platform: platform.to_string(),
                    model_id: ext.id.clone(),
                    display_name: ext.name.clone(),
                    context_window: Some(ext.context_window),
                    pricing_prompt: ext.pricing.as_ref().map(|p| p.prompt),
                    pricing_completion: ext.pricing.as_ref().map(|p| p.completion),
                    free_tier: if is_free { 1 } else { 0 },
                    gateway: ext.gateway.clone(),
                    supported_features: serde_json::to_value(&ext.supported_features)
                        .unwrap_or(serde_json::Value::Array(vec![]))
                        .to_string(),
                    external_url: ext.url.clone(),
                    description: ext.description.clone(),
                    last_synced_at: started_at.to_string(),
                    enabled: if is_free { 1 } else { 0 },
                });

                if is_free {
                    result.added += 1;
                    result.changes.push(SyncChange {
                        change_type: "added".to_string(),
                        platform: platform.to_string(),
                        model_id: ext.id.clone(),
                        display_name: ext.name.clone(),
                        details: None,
                    });
                } else {
                    result.stored_disabled += 1;
                    result.changes.push(SyncChange {
                        change_type: "stored".to_string(),
                        platform: platform.to_string(),
                        model_id: ext.id.clone(),
                        display_name: ext.name.clone(),
                        details: None,
                    });
                }
            }
            Some(existing) => {
                let old_free_tier = existing.free_tier == 1;

                if old_free_tier && !is_free {
                    result.free_to_paid += 1;
                    result.changes.push(SyncChange {
                        change_type: "free_to_paid".to_string(),
                        platform: platform.to_string(),
                        model_id: ext.id.clone(),
                        display_name: ext.name.clone(),
                        details: Some(
                            serde_json::json!({
                                "before": { "freeTier": true },
                                "after": { "freeTier": false },
                            })
                            .to_string(),
                        ),
                    });
                    disables.push(existing.id);
                    continue;
                }

                if !old_free_tier && is_free {
                    result.paid_to_free += 1;
                    result.changes.push(SyncChange {
                        change_type: "paid_to_free".to_string(),
                        platform: platform.to_string(),
                        model_id: ext.id.clone(),
                        display_name: ext.name.clone(),
                        details: None,
                    });
                }

                updates.push(UpdatePlan {
                    id: existing.id,
                    display_name: ext.name.clone(),
                    context_window: Some(ext.context_window),
                    pricing_prompt: ext.pricing.as_ref().map(|p| p.prompt),
                    pricing_completion: ext.pricing.as_ref().map(|p| p.completion),
                    free_tier: if is_free { 1 } else { 0 },
                    gateway: ext.gateway.clone(),
                    supported_features: serde_json::to_value(&ext.supported_features)
                        .unwrap_or(serde_json::Value::Array(vec![]))
                        .to_string(),
                    external_url: ext.url.clone(),
                    description: ext.description.clone(),
                });
                result.updated += 1;
            }
        }
    }

    for (key, model) in &current_map {
        if !seen_keys.contains(key) && model.source == "getmodelsapi" && model.enabled == 1 {
            disables.push(model.id);
            result.disabled += 1;
            result.changes.push(SyncChange {
                change_type: "disabled".to_string(),
                platform: model.platform.clone(),
                model_id: model.model_id.clone(),
                display_name: model.display_name.clone(),
                details: None,
            });
        }
    }

    {
        // Keep catalog mutations and completion log writes in one transaction.
        // The connection lock is held only for synchronous SQL.
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for ins in &new_model_inserts {
            tx.execute(
                "INSERT OR IGNORE INTO models \
                 (platform, model_id, display_name, intelligence_rank, speed_rank, size_label, \
                  rpm_limit, rpd_limit, tpm_limit, tpd_limit, monthly_token_budget, context_window, \
                  pricing_prompt, pricing_completion, free_tier, gateway, supported_features, \
                  external_url, description, last_synced_at, source, enabled) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,'getmodelsapi',?21)",
                rusqlite::params![
                    ins.platform,
                    ins.model_id,
                    ins.display_name,
                    CURATION_DEFAULTS.intelligence_rank,
                    CURATION_DEFAULTS.speed_rank,
                    CURATION_DEFAULTS.size_label,
                    CURATION_DEFAULTS.rpm_limit,
                    CURATION_DEFAULTS.rpd_limit,
                    CURATION_DEFAULTS.tpm_limit,
                    CURATION_DEFAULTS.tpd_limit,
                    CURATION_DEFAULTS.monthly_token_budget,
                    ins.context_window,
                    ins.pricing_prompt,
                    ins.pricing_completion,
                    ins.free_tier,
                    ins.gateway,
                    ins.supported_features,
                    ins.external_url,
                    ins.description,
                    ins.last_synced_at,
                    ins.enabled,
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        for up in &updates {
            tx.execute(
                "UPDATE models SET display_name = ?1, context_window = ?2, pricing_prompt = ?3, \
                 pricing_completion = ?4, free_tier = ?5, gateway = ?6, supported_features = ?7, \
                 external_url = ?8, description = ?9, last_synced_at = ?10 WHERE id = ?11",
                rusqlite::params![
                    up.display_name,
                    up.context_window,
                    up.pricing_prompt,
                    up.pricing_completion,
                    up.free_tier,
                    up.gateway,
                    up.supported_features,
                    up.external_url,
                    up.description,
                    started_at,
                    up.id
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        for id in &disables {
            tx.execute(
                "UPDATE models SET enabled = 0, last_synced_at = ?1 WHERE id = ?2",
                rusqlite::params![started_at, id],
            )
            .map_err(|e| e.to_string())?;
        }
        // Only add free + enabled models to fallback_config
        for ins in &new_model_inserts {
            if ins.enabled == 0 {
                continue;
            }
            let inserted_id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM models WHERE platform = ?1 AND model_id = ?2",
                    rusqlite::params![ins.platform, ins.model_id],
                    |r| r.get(0),
                )
                .ok();
            let Some(inserted_id) = inserted_id else {
                continue;
            };
            let exists: Option<i64> = tx
                .query_row(
                    "SELECT id FROM fallback_config WHERE model_db_id = ?1",
                    rusqlite::params![inserted_id],
                    |r| r.get(0),
                )
                .ok();
            if exists.is_none() {
                let max_p: Option<i64> = tx
                    .query_row("SELECT MAX(priority) FROM fallback_config", [], |r| {
                        r.get(0)
                    })
                    .ok()
                    .flatten();
                tx.execute(
                    "INSERT INTO fallback_config (model_db_id, priority, enabled) VALUES (?1, ?2, 1)",
                    rusqlite::params![inserted_id, max_p.unwrap_or(0) + 1],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        insert_sync_changes(&tx, log_id, &result.changes);

        tx.execute(
            "UPDATE sync_log SET completed_at = ?1, status = 'completed', total_discovered = ?2, \
             added = ?3, updated = ?4, disabled = ?5, free_to_paid = ?6, paid_to_free = ?7, \
             stored_disabled = ?8 WHERE id = ?9",
            rusqlite::params![
                now_iso(),
                result.total_discovered,
                result.added,
                result.updated,
                result.disabled,
                result.free_to_paid,
                result.paid_to_free,
                result.stored_disabled,
                log_id
            ],
        )
        .map_err(|e| e.to_string())?;

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
    }

    // Refresh intelligence/speed rankings from real benchmark sources.
    // Failures are non-fatal — sync succeeded, enrichment is best-effort and
    // can be re-triggered via POST /api/models/enrich-rankings.
    let enrich_result = enrich_rankings().await;
    tracing::info!(
        "[ModelSync] Enrichment done: {}/{} ranked from {} in {}ms",
        enrich_result.updated,
        enrich_result.scanned,
        enrich_result.source,
        enrich_result.duration_ms
    );
    result.enrich = Some(EnrichSummary {
        scanned: enrich_result.scanned,
        updated: enrich_result.updated,
        skipped: enrich_result.skipped,
        source: enrich_result.source,
        duration_ms: enrich_result.duration_ms,
        errors: enrich_result.errors,
    });

    Ok(result)
}

fn insert_sync_changes(conn: &Connection, log_id: i64, changes: &[SyncChange]) {
    for change in changes {
        conn.execute(
            "INSERT INTO sync_changes \
             (sync_log_id, change_type, platform, model_id, display_name, details) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                log_id,
                change.change_type,
                change.platform,
                change.model_id,
                change.display_name,
                change.details
            ],
        )
        .ok();
    }
}
