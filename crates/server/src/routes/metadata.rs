//! All metadata and simulation routes are behind the existing admin-auth layer.
use crate::{
    db::connection::db,
    error::ApiError,
    services::model_metadata::{self as metadata, persistence, sources},
};
use axum::{
    extract::{Path, Query},
    routing::{get, patch, post},
    Json,
};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
fn sql_error(_: rusqlite::Error) -> ApiError {
    ApiError::new(500, "Metadata database operation failed")
}
pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/sources", get(list_sources))
        .route("/sources/{id}", patch(update_source))
        .route("/sync", post(sync))
        .route("/sync/status", get(sync_status))
        .route("/matches", get(matches))
        .route("/matches/{id}/resolve", post(resolve))
        .route("/matches/{id}/audit", get(match_audit))
        .route("/explorer", get(explorer))
        .route("/definitions", get(definitions))
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
    query: Option<String>,
    provider: Option<String>,
    capability: Option<String>,
    free: Option<bool>,
    min_context: Option<i64>,
    sort: Option<String>,
    status: Option<String>,
    metric: Option<String>,
    window: Option<i64>,
    ids: Option<String>,
}
impl Page {
    fn bounds(&self) -> (i64, i64) {
        (
            self.limit.unwrap_or(50).clamp(1, 200),
            self.offset.unwrap_or(0).clamp(0, 1_000_000),
        )
    }
}
pub async fn list_sources() -> Result<Json<Value>, ApiError> {
    let conn = db().lock().await;
    let mut result = Vec::new();
    for s in sources::registry() {
        let d = s.definition();
        let row=conn.query_row("SELECT enabled,ttl_seconds,status,last_success,last_failure,last_error,next_attempt FROM metadata_sources WHERE id=?1",[d.id],|r|Ok(json!({"id":d.id,"enabled":r.get::<_,bool>(0)?,"ttlSeconds":r.get::<_,i64>(1)?,"status":r.get::<_,String>(2)?,"lastSuccess":r.get::<_,Option<String>>(3)?,"lastFailure":r.get::<_,Option<String>>(4)?,"error":r.get::<_,Option<String>>(5)?,"nextAttempt":r.get::<_,i64>(6)?,"configured":d.configured,"attribution":d.attribution,"license":d.license,"domains":d.domains}))).map_err(sql_error)?;
        result.push(row);
    }
    Ok(Json(json!(result)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePatch {
    enabled: Option<bool>,
    ttl_seconds: Option<i64>,
}
async fn update_source(
    Path(id): Path<String>,
    Json(p): Json<SourcePatch>,
) -> Result<Json<Value>, ApiError> {
    if p.ttl_seconds.is_some_and(|v| !(300..=604800).contains(&v)) {
        return Err(ApiError::new(400, "ttlSeconds must be 300..604800"));
    }
    let mut conn = db().lock().await;
    let tx = conn.transaction().map_err(sql_error)?;
    let n=tx.execute("UPDATE metadata_sources SET enabled=COALESCE(?1,enabled),ttl_seconds=COALESCE(?2,ttl_seconds) WHERE id=?3",params![p.enabled,p.ttl_seconds,id]).map_err(sql_error)?;
    if n == 0 {
        return Err(ApiError::new(404, "Unknown metadata source"));
    }
    persistence::recompute(&tx).map_err(sql_error)?;
    tx.commit().map_err(sql_error)?;
    Ok(Json(json!({"ok":true})))
}
async fn sync() -> Json<Value> {
    Json(json!({"sources":metadata::refresh::refresh(true).await}))
}
async fn sync_status() -> Result<Json<Value>, ApiError> {
    let conn = db().lock().await;
    let mut stmt=conn.prepare("SELECT id,source,started_at,completed_at,status,summary_json FROM metadata_sync_runs ORDER BY id DESC LIMIT 100").map_err(sql_error)?;
    let rows=stmt.query_map([],|r|{let summary:String=r.get(5)?;Ok(json!({"id":r.get::<_,i64>(0)?,"source":r.get::<_,String>(1)?,"startedAt":r.get::<_,String>(2)?,"completedAt":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"summary":serde_json::from_str::<Value>(&summary).unwrap_or(Value::Null)}))}).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    Ok(Json(json!(rows)))
}
async fn matches(Query(p): Query<Page>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = p.bounds();
    let status = p.status.unwrap_or_else(|| "unmatched".into());
    if ![
        "unmatched",
        "ambiguous",
        "matched",
        "rejected",
        "canonicalOnly",
    ]
    .contains(&status.as_str())
    {
        return Err(ApiError::new(400, "Invalid match status"));
    }
    let conn = db().lock().await;
    let mut stmt=conn.prepare("SELECT MIN(id),source,source_model_id,model_id,provider,canonical_hint,COUNT(*) FROM model_metadata_observations WHERE match_status=?1 GROUP BY source,source_model_id ORDER BY source,source_model_id LIMIT ?2 OFFSET ?3").map_err(sql_error)?;
    let rows=stmt.query_map(params![status,limit,offset],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"source":r.get::<_,String>(1)?,"sourceModelId":r.get::<_,String>(2)?,"modelId":r.get::<_,String>(3)?,"provider":r.get::<_,Option<String>>(4)?,"canonicalHint":r.get::<_,Option<String>>(5)?,"observations":r.get::<_,i64>(6)?}))).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    Ok(Json(json!({"data":rows,"limit":limit,"offset":offset})))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolution {
    canonical_model_id: Option<i64>,
    reject: Option<bool>,
    reason: String,
}
async fn resolve(Path(id): Path<i64>, Json(p): Json<Resolution>) -> Result<Json<Value>, ApiError> {
    if p.reason.trim().is_empty()
        || p.reason.len() > 1000
        || (p.reject == Some(true)) == p.canonical_model_id.is_some()
    {
        return Err(ApiError::new(
            400,
            "Provide a reason and either reject=true or canonicalModelId",
        ));
    }
    let mut conn = db().lock().await;
    let tx = conn.transaction().map_err(sql_error)?;
    let(source,key,old):(String,String,Option<i64>)=tx.query_row("SELECT source,source_model_id,canonical_model_id FROM model_metadata_observations WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql_error)?.ok_or_else(||ApiError::new(404,"Observation not found"))?;
    if let Some(c) = p.canonical_model_id {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM canonical_models WHERE id=?1)",
                [c],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Err(ApiError::new(400, "Unknown canonical model"));
        }
    }
    let decision = if p.reject == Some(true) {
        "rejected"
    } else {
        "matched"
    };
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute("INSERT INTO metadata_match_decisions(source,source_model_id,decision,canonical_model_id,reason,updated_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source,source_model_id) DO UPDATE SET decision=excluded.decision,canonical_model_id=excluded.canonical_model_id,reason=excluded.reason,updated_at=excluded.updated_at",params![source,key,decision,p.canonical_model_id,p.reason,now]).map_err(sql_error)?;
    tx.execute("INSERT INTO metadata_match_audit(source,source_model_id,decision,previous_canonical_id,canonical_model_id,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![source,key,decision,old,p.canonical_model_id,p.reason,now]).map_err(sql_error)?;
    // Only this explicit, audited admin operation may replace a verified alias.
    if let Some(c) = p.canonical_model_id {
        tx.execute("INSERT INTO model_aliases(canonical_model_id,source,source_model_id,confidence,verified) VALUES(?1,?2,?3,1,1) ON CONFLICT(source,source_model_id) DO UPDATE SET canonical_model_id=excluded.canonical_model_id,confidence=1,verified=1",params![c,source,key]).map_err(sql_error)?;
    }
    tx.execute("UPDATE model_metadata_observations SET canonical_model_id=?1,match_status=CASE WHEN ?2='rejected' THEN 'rejected' WHEN scope='canonicalModel' OR deployment_id IS NOT NULL THEN 'matched' ELSE 'canonicalOnly' END,target_key=CASE WHEN ?2='rejected' THEN '' WHEN deployment_id IS NOT NULL THEN 'd:'||deployment_id WHEN scope='canonicalModel' THEN 'c:'||?1 ELSE '' END WHERE source=?3 AND source_model_id=?4",params![p.canonical_model_id,decision,source,key]).map_err(sql_error)?;
    if let Some(c) = p.canonical_model_id {
        tx.execute("UPDATE models SET canonical_model_id=?1 WHERE id IN (SELECT deployment_id FROM model_metadata_observations WHERE source=?2 AND source_model_id=?3 AND deployment_id IS NOT NULL)",params![c,source,key]).map_err(sql_error)?;
        tx.execute("UPDATE model_metadata_observations SET canonical_model_id=?1 WHERE deployment_id IN (SELECT deployment_id FROM model_metadata_observations WHERE source=?2 AND source_model_id=?3 AND deployment_id IS NOT NULL)",params![c,source,key]).map_err(sql_error)?;
    }
    persistence::recompute(&tx).map_err(sql_error)?;
    tx.commit().map_err(sql_error)?;
    Ok(Json(json!({"ok":true})))
}
pub async fn detail(Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = db().lock().await;
    let metrics = persistence::metrics_for(&conn, id).map_err(|e| {
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            ApiError::new(404, "Model not found")
        } else {
            sql_error(e)
        }
    })?;
    let (platform, model): (String, String) = conn
        .query_row(
            "SELECT platform,model_id FROM models WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql_error)?;
    let performance = crate::services::telemetry::load_performance(&conn, &platform, &model)
        .map_err(sql_error)?;
    Ok(Json(
        json!({"id":id,"metrics":metrics,"candidates":persistence::candidates_for(&conn,id).map_err(sql_error)?,"performance":performance,"windows":crate::services::telemetry::rolling_summaries(&conn,&platform,&model).map_err(sql_error)?}),
    ))
}
pub async fn history(Path(id): Path<i64>, Query(p): Query<Page>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = p.bounds();
    let days = p.window.unwrap_or(7).clamp(1, 90);
    let conn = db().lock().await;
    let canonical: Option<i64> = conn
        .query_row(
            "SELECT canonical_model_id FROM models WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| ApiError::new(404, "Model not found"))?;
    let mut stmt=conn.prepare("SELECT metric,value_json,source,unit,benchmark_version,changed_at FROM model_metric_history WHERE (target_key=?1 OR target_key=?2) AND (?3 IS NULL OR metric=?3) AND changed_at>=?4 ORDER BY changed_at DESC,id DESC LIMIT ?5 OFFSET ?6").map_err(sql_error)?;
    let rows=stmt.query_map(params![format!("d:{id}"),canonical.map(|c|format!("c:{c}")).unwrap_or_default(),p.metric,(chrono::Utc::now()-chrono::Duration::days(days)).to_rfc3339(),limit,offset],|r|{let v:String=r.get(1)?;Ok(json!({"metric":r.get::<_,String>(0)?,"value":serde_json::from_str::<Value>(&v).unwrap_or(Value::Null),"source":r.get::<_,String>(2)?,"unit":r.get::<_,String>(3)?,"benchmarkVersion":r.get::<_,String>(4)?,"changedAt":r.get::<_,String>(5)?}))}).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    Ok(Json(json!({"data":rows,"limit":limit,"offset":offset})))
}
pub async fn compare(Query(p): Query<Page>) -> Result<Json<Value>, ApiError> {
    let ids = p
        .ids
        .unwrap_or_default()
        .split(',')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApiError::new(400, "Invalid ids"))?;
    if !(2..=4).contains(&ids.len()) || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::new(400, "Compare 2..4 positive model ids"));
    }
    let conn = db().lock().await;
    let rows = ids
        .into_iter()
        .map(|id| persistence::metrics_for(&conn, id).map(|m| json!({"id":id,"metrics":m})))
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok(Json(json!({"data":rows})))
}
async fn definitions() -> Json<Value> {
    Json(
        json!({"benchmarkComparison":"Compare only the same source, metric and version. Unknown is not zero.","scopes":["canonicalModel","providerDeployment","localObservation"]}),
    )
}
async fn explorer(Query(p): Query<Page>) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = p.bounds();
    let conn = db().lock().await;
    let mut conditions = vec!["1=1".to_string()];
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(q) = p.query.filter(|v| !v.is_empty()) {
        if q.len() > 200 {
            return Err(ApiError::new(400, "Query is too long"));
        }
        let fts: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='model_catalog_fts')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if fts && q.split_whitespace().next().is_some() {
            let expression = q
                .split_whitespace()
                .map(|word| format!("\"{}\"*", word.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" AND ");
            conditions.push(
                "m.id IN (SELECT rowid FROM model_catalog_fts WHERE model_catalog_fts MATCH ?)"
                    .into(),
            );
            args.push(expression.into());
        } else {
            conditions.push("(m.display_name LIKE ? OR m.model_id LIKE ? OR m.description LIKE ? OR EXISTS(SELECT 1 FROM model_aliases a WHERE a.canonical_model_id=m.canonical_model_id AND a.source_model_id LIKE ?))".into());
            for _ in 0..4 {
                args.push(format!("%{q}%").into());
            }
        }
    }

    if let Some(provider) = p.provider.filter(|v| !v.is_empty()) {
        conditions.push("m.platform=?".into());
        args.push(provider.into());
    }
    if let Some(free) = p.free {
        conditions.push("m.free_tier=?".into());
        args.push(i64::from(free).into());
    }
    if let Some(min) = p.min_context {
        conditions.push("COALESCE((SELECT json_extract(o.value_json,'$.value') FROM model_metadata_effective e JOIN model_metadata_observations o ON o.id=e.observation_id WHERE e.target_key='d:'||m.id AND e.metric='context.total'),m.context_window)>=?".into());
        args.push(min.max(0).into());
    }
    if let Some(cap) = p.capability.filter(|v| !v.is_empty()) {
        if ![
            "tools",
            "vision",
            "reasoning",
            "structured_output",
            "audio_input",
            "video_input",
        ]
        .contains(&cap.as_str())
        {
            return Err(ApiError::new(400, "Unknown capability"));
        }
        conditions.push("EXISTS(SELECT 1 FROM model_metadata_effective e JOIN model_metadata_observations o ON o.id=e.observation_id WHERE e.target_key='d:'||m.id AND e.metric=? AND json_extract(o.value_json,'$.value')=1)".into());
        args.push(format!("capability.{cap}").into());
    }
    let sort = match p.sort.as_deref() {
        Some("context") => "MAX(m.context_window) DESC",
        Some("quality") => "MAX(m.intelligence_score) DESC",
        Some("recency") => "MAX(m.last_synced_at) DESC",
        Some("coding") => "MAX((SELECT json_extract(o.value_json,'$.value') FROM model_metadata_effective e JOIN model_metadata_observations o ON o.id=e.observation_id WHERE e.target_key='c:'||m.canonical_model_id AND e.metric='benchmark.aa.coding')) DESC",
        Some("speed") => "MAX((SELECT ewma_output_tps FROM model_performance WHERE platform=m.platform AND model_id=m.model_id AND sample_count>=5)) DESC",
        Some("ttft") => "MIN((SELECT ewma_ttft_ms FROM model_performance WHERE platform=m.platform AND model_id=m.model_id AND sample_count>=5)) ASC NULLS LAST",
        Some("price") => "MIN((SELECT json_extract(o.value_json,'$.value') FROM model_metadata_effective e JOIN model_metadata_observations o ON o.id=e.observation_id WHERE e.target_key='d:'||m.id AND e.metric='price.input')) ASC NULLS LAST",
        Some("reliability") => "MAX((SELECT (success_count+8.0)/(sample_count+10.0) FROM model_performance WHERE platform=m.platform AND model_id=m.model_id AND sample_count>=5)) DESC",
        _ => "MIN(m.display_name) COLLATE NOCASE",
    };
    let condition = conditions.join(" AND ");
    let total:i64=conn.query_row(&format!("SELECT COUNT(DISTINCT COALESCE('c:'||m.canonical_model_id,'d:'||m.id)) FROM models m WHERE {condition}"),rusqlite::params_from_iter(args.iter()),|r|r.get(0)).map_err(sql_error)?;
    args.push(limit.into());
    args.push(offset.into());
    let mut stmt=conn.prepare(&format!("SELECT COALESCE('c:'||m.canonical_model_id,'d:'||m.id),MIN(m.display_name),group_concat(m.id) FROM models m WHERE {condition} GROUP BY COALESCE('c:'||m.canonical_model_id,'d:'||m.id) ORDER BY {sort},MIN(m.id) LIMIT ? OFFSET ?")).map_err(sql_error)?;
    let groups = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut data = Vec::new();
    for (key, name, ids) in groups {
        let mut deployments = Vec::new();
        for id in ids
            .split(',')
            .take(200)
            .filter_map(|s| s.parse::<i64>().ok())
        {
            let mut row:Value=conn.query_row("SELECT id,platform,model_id,enabled,free_tier FROM models WHERE id=?1",[id],|r|Ok(json!({"id":id,"provider":r.get::<_,String>(1)?,"modelId":r.get::<_,String>(2)?,"enabled":r.get::<_,bool>(3)?,"freeTier":r.get::<_,bool>(4)?}))).map_err(sql_error)?;
            row["metrics"] = persistence::metrics_for(&conn, id).map_err(sql_error)?;
            deployments.push(row);
        }
        data.push(json!({"key":key,"name":name,"deployments":deployments}));
    }
    Ok(Json(
        json!({"data":data,"total":total,"limit":limit,"offset":offset}),
    ))
}

pub async fn simulate(
    Json(p): Json<metadata::routing::RoutingPolicy>,
) -> Result<Json<Value>, ApiError> {
    p.validate().map_err(|e| ApiError::new(400, e))?;
    let conn = db().lock().await;
    let p = metadata::routing::configured_policy(&conn, &p);
    let mut stmt=conn.prepare("SELECT model_db_id,COALESCE(manual_priority,priority) FROM fallback_config ORDER BY priority,id LIMIT 5000").map_err(sql_error)?;
    let ids = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut decisions = ids
        .into_iter()
        .map(|(id, priority)| metadata::routing::evaluate(&conn, id, &p).map(|d| (priority, d)))
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    decisions.sort_by(|a, b| {
        b.1.eligible
            .cmp(&a.1.eligible)
            .then_with(|| {
                b.1.score
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&a.1.score.unwrap_or(f64::NEG_INFINITY))
            })
            .then(a.0.cmp(&b.0))
            .then(a.1.model_db_id.cmp(&b.1.model_db_id))
    });
    let selected = decisions
        .iter()
        .find(|(_, d)| d.eligible)
        .map(|(_, d)| d.model_db_id);
    Ok(Json(
        json!({"selectedModelId":selected,"decisions":decisions.into_iter().map(|(_,d)|d).collect::<Vec<_>>()}),
    ))
}

pub async fn get_policy() -> Result<Json<Value>, ApiError> {
    let conn = db().lock().await;
    let p: metadata::routing::RoutingPolicy = conn
        .query_row(
            "SELECT value FROM settings WHERE key='routing_policy'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    Ok(Json(json!(p)))
}
pub async fn set_policy(
    Json(p): Json<metadata::routing::RoutingPolicy>,
) -> Result<Json<Value>, ApiError> {
    p.validate().map_err(|e| ApiError::new(400, e))?;
    let conn = db().lock().await;
    conn.execute("INSERT INTO settings(key,value) VALUES('routing_policy',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&p).unwrap_or_default()]).map_err(sql_error)?;
    Ok(Json(json!({"ok":true})))
}

async fn match_audit(Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = db().lock().await;
    let (source, key): (String, String) = conn
        .query_row(
            "SELECT source,source_model_id FROM model_metadata_observations WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| ApiError::new(404, "Observation not found"))?;
    let mut stmt=conn.prepare("SELECT id,decision,previous_canonical_id,canonical_model_id,reason,created_at FROM metadata_match_audit WHERE source=?1 AND source_model_id=?2 ORDER BY id DESC LIMIT 100").map_err(sql_error)?;
    let rows=stmt.query_map(params![source,key],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"decision":r.get::<_,String>(1)?,"previousCanonicalId":r.get::<_,Option<i64>>(2)?,"canonicalModelId":r.get::<_,Option<i64>>(3)?,"reason":r.get::<_,String>(4)?,"createdAt":r.get::<_,String>(5)?}))).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    Ok(Json(json!({"data":rows})))
}
