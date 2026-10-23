use super::{matching::MatchIndex, metrics, sources, types::*};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(include_str!("schema.sql"))?;
    for source in sources::registry() {
        let d = source.definition();
        conn.execute(
            "INSERT OR IGNORE INTO metadata_sources(id,enabled,ttl_seconds) VALUES (?1,?2,?3)",
            params![d.id, d.default_enabled, d.ttl_seconds],
        )?;
    }
    // FTS is optional. Triggers update in the same transaction as catalog/alias mutations.
    if conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS model_catalog_fts USING fts5(name,model_id,description,aliases);").is_ok() {
        conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS model_catalog_fts USING fts5(name,model_id,description,aliases);
        CREATE TRIGGER IF NOT EXISTS metadata_models_insert AFTER INSERT ON models BEGIN
          INSERT INTO model_catalog_fts(rowid,name,model_id,description,aliases) VALUES(new.id,new.display_name,new.model_id,new.description,''); END;
        CREATE TRIGGER IF NOT EXISTS metadata_models_delete AFTER DELETE ON models BEGIN DELETE FROM model_catalog_fts WHERE rowid=old.id; END;
        CREATE TRIGGER IF NOT EXISTS metadata_models_update AFTER UPDATE OF display_name,model_id,description,canonical_model_id ON models BEGIN
          DELETE FROM model_catalog_fts WHERE rowid=old.id;
          INSERT INTO model_catalog_fts(rowid,name,model_id,description,aliases) VALUES(new.id,new.display_name,new.model_id,new.description,(SELECT group_concat(source_model_id,' ') FROM model_aliases WHERE canonical_model_id=new.canonical_model_id)); END;
        CREATE TRIGGER IF NOT EXISTS metadata_alias_insert AFTER INSERT ON model_aliases BEGIN
          UPDATE model_catalog_fts SET aliases=(SELECT group_concat(source_model_id,' ') FROM model_aliases WHERE canonical_model_id=new.canonical_model_id) WHERE rowid IN (SELECT id FROM models WHERE canonical_model_id=new.canonical_model_id); END;
        CREATE TRIGGER IF NOT EXISTS metadata_alias_update AFTER UPDATE ON model_aliases BEGIN
          UPDATE model_catalog_fts SET aliases=(SELECT group_concat(a.source_model_id,' ') FROM model_aliases a JOIN models m ON m.canonical_model_id=a.canonical_model_id WHERE m.id=model_catalog_fts.rowid) WHERE rowid IN (SELECT id FROM models WHERE canonical_model_id IN (old.canonical_model_id,new.canonical_model_id)); END;
        CREATE TRIGGER IF NOT EXISTS metadata_alias_delete AFTER DELETE ON model_aliases BEGIN
          UPDATE model_catalog_fts SET aliases=(SELECT group_concat(source_model_id,' ') FROM model_aliases WHERE canonical_model_id=old.canonical_model_id) WHERE rowid IN (SELECT id FROM models WHERE canonical_model_id=old.canonical_model_id); END;
        INSERT INTO model_catalog_fts(rowid,name,model_id,description,aliases) SELECT id,display_name,model_id,description,(SELECT group_concat(source_model_id,' ') FROM model_aliases WHERE canonical_model_id=models.canonical_model_id) FROM models WHERE id NOT IN (SELECT rowid FROM model_catalog_fts);")?;
    }
    Ok(())
}
pub fn recompute(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM model_metadata_effective", [])?;
    conn.execute_batch("INSERT INTO model_metadata_effective(target_key,metric,observation_id,selection_reason)
    SELECT target_key,metric,id,'verified, field authority, freshness, confidence, stable source ID' FROM (
      SELECT o.*,ROW_NUMBER() OVER (PARTITION BY target_key,metric ORDER BY
        verified DESC,
        CASE WHEN datetime(fetched_at)>=datetime('now','-'||s.ttl_seconds||' seconds') THEN 1 ELSE 0 END DESC,
        CASE WHEN source='manual' THEN 100 WHEN scope='providerDeployment' AND source='openrouter' THEN 90
             WHEN source='artificial-analysis' AND metric LIKE 'benchmark.%' THEN 90
             WHEN source='models-dev' THEN 70 WHEN source='litellm' THEN 40 ELSE 50 END DESC,
        fetched_at DESC,confidence DESC,source ASC,o.id ASC) AS position
      FROM model_metadata_observations o JOIN metadata_sources s ON s.id=o.source
      WHERE s.enabled=1 AND match_status='matched' AND target_key!=''
    ) WHERE position=1;")?;
    Ok(())
}
pub fn ingest(
    conn: &mut Connection,
    source: &str,
    rows: &[MetadataObservation],
) -> rusqlite::Result<SyncResult> {
    crate::services::rankings::enrich::ensure_identity_registry(conn)?;
    let index = MatchIndex::load(conn)?;
    let tx = conn.transaction()?;
    let mut result = SyncResult {
        source: source.into(),
        status: "fetched".into(),
        fetched: rows.len(),
        ..Default::default()
    };
    for o in rows {
        if o.source != source || !metrics::validate(o) {
            continue;
        }
        result.valid += 1;
        let matched = index.resolve(o);
        match matched.status {
            "matched" => result.matched += 1,
            "ambiguous" => result.ambiguous += 1,
            _ => result.unmatched += 1,
        };
        let target = if let Some(id) = matched.deployment {
            format!("d:{id}")
        } else if o.scope == MetricScope::CanonicalModel {
            matched
                .canonical
                .map(|id| format!("c:{id}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let val = serde_json::to_string(&o.value).expect("validated value");
        let version = o.benchmark_version.as_deref().unwrap_or("");
        let prev:Option<String>=tx.prepare_cached("SELECT value_json FROM model_metadata_observations WHERE source=?1 AND source_model_id=?2 AND scope=?3 AND metric=?4 AND benchmark_version=?5")?.query_row(params![source,o.source_model_id,o.scope.key(),o.metric,version],|r|r.get(0)).optional()?;
        if prev.as_deref() != Some(&val) {
            result.changed += 1;
            if !target.is_empty() {
                tx.prepare_cached("INSERT INTO model_metric_history(target_key,source,metric,value_json,unit,benchmark_version,changed_at) VALUES(?1,?2,?3,?4,?5,?6,?7)")?.execute(params![target,source,o.metric,val,o.unit,version,o.fetched_at.to_rfc3339()])?;
            }
        }
        tx.prepare_cached("INSERT INTO model_metadata_observations(source,source_model_id,model_id,provider,canonical_hint,canonical_model_id,deployment_id,target_key,metric,scope,value_json,unit,fetched_at,observed_at,benchmark_version,confidence,verified,original_json,match_status)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
        ON CONFLICT(source,source_model_id,scope,metric,benchmark_version) DO UPDATE SET canonical_model_id=excluded.canonical_model_id,deployment_id=excluded.deployment_id,target_key=excluded.target_key,value_json=excluded.value_json,unit=excluded.unit,fetched_at=excluded.fetched_at,observed_at=excluded.observed_at,confidence=excluded.confidence,original_json=excluded.original_json,match_status=excluded.match_status
        WHERE model_metadata_observations.verified=0")?.execute(params![source,o.source_model_id,o.model_id,o.provider,o.canonical_hint,matched.canonical,matched.deployment,target,o.metric,o.scope.key(),val,o.unit,o.fetched_at.to_rfc3339(),o.observed_at,version,o.confidence,o.verified,o.original_value.as_ref().map(Value::to_string),matched.status])?;
    }
    recompute(&tx)?;
    tx.execute_batch("DELETE FROM model_metric_history WHERE changed_at<strftime('%Y-%m-%dT%H:%M:%fZ','now','-'||max(1,min(COALESCE(CAST((SELECT value FROM settings WHERE key='metadata_history_retention_days')AS INTEGER),90),365))||' days');
    DELETE FROM model_metric_history WHERE id IN (SELECT id FROM model_metric_history ORDER BY id DESC LIMIT -1 OFFSET 100000);
    DELETE FROM metadata_sync_runs WHERE id IN (SELECT id FROM metadata_sync_runs ORDER BY id DESC LIMIT -1 OFFSET 1000);")?;
    tx.commit()?;
    Ok(result)
}
pub fn metrics_for(conn: &Connection, id: i64) -> rusqlite::Result<Value> {
    let canonical: Option<i64> = conn.query_row(
        "SELECT canonical_model_id FROM models WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    let mut stmt=conn.prepare("SELECT o.id,o.metric,o.value_json,o.unit,o.source,o.scope,o.confidence,o.fetched_at,o.observed_at,o.benchmark_version,o.verified,e.selection_reason,s.ttl_seconds,
    (SELECT COUNT(DISTINCT value_json) FROM model_metadata_observations x JOIN metadata_sources xs ON xs.id=x.source WHERE x.target_key=o.target_key AND x.metric=o.metric AND x.match_status='matched' AND xs.enabled=1)
    FROM model_metadata_effective e JOIN model_metadata_observations o ON o.id=e.observation_id JOIN metadata_sources s ON s.id=o.source WHERE e.target_key=?1 OR e.target_key=?2 ORDER BY o.metric,o.scope")?;
    let mut rows=stmt.query_map(params![format!("d:{id}"),canonical.map(|c|format!("c:{c}")).unwrap_or_default()],|r| {
        let fetched:String=r.get(7)?;let ttl:i64=r.get(12)?;
        let fresh=chrono::DateTime::parse_from_rfc3339(&fetched).is_ok_and(|d|(chrono::Utc::now()-d.with_timezone(&chrono::Utc)).num_seconds()<ttl);
        let value:String=r.get(2)?;
        Ok(json!({"id":r.get::<_,i64>(0)?,"metric":r.get::<_,String>(1)?,"value":serde_json::from_str::<Value>(&value).unwrap_or(Value::Null),"unit":r.get::<_,String>(3)?,"source":r.get::<_,String>(4)?,"scope":r.get::<_,String>(5)?,"confidence":r.get::<_,f64>(6)?,"fetchedAt":fetched,"observedAt":r.get::<_,Option<String>>(8)?,"benchmarkVersion":r.get::<_,String>(9)?,"verified":r.get::<_,bool>(10)?,"selectionReason":r.get::<_,String>(11)?,"freshness":if fresh{"fresh"}else{"stale"},"conflict":r.get::<_,i64>(13)?>1}))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    // Existing reviewed overrides retain authority without migrating or deleting legacy rows.
    if let Some(c) = canonical {
        let override_row=conn.query_row("SELECT intelligence_score,speed_tokens_per_sec,confidence,fetched_at FROM model_benchmarks WHERE canonical_model_id=?1 AND source='manual'",[c],|r|Ok((r.get::<_,Option<f64>>(0)?,r.get::<_,Option<f64>>(1)?,r.get::<_,f64>(2)?,r.get::<_,String>(3)?))).optional()?;
        if let Some((quality, speed, confidence, date)) = override_row {
            for (key, value, unit) in [
                ("benchmark.manual.intelligence", quality, "index"),
                ("performance.output_tps", speed, "tokens/s"),
            ] {
                if let Some(value) = value {
                    rows.retain(|r| !(r["metric"] == key && r["scope"] == "canonicalModel"));
                    rows.push(json!({"id":-id,"metric":key,"value":{"type":"number","value":value},"unit":unit,"source":"manual","scope":"canonicalModel","confidence":confidence,"fetchedAt":date,"observedAt":date,"benchmarkVersion":"manual","verified":true,"freshness":"fresh","conflict":false,"selectionReason":"reviewed legacy manual override"}));
                }
            }
        }
    }
    let performance=conn.query_row("SELECT p.ewma_output_tps,p.ewma_ttft_ms,p.ewma_latency_ms,p.sample_count,p.updated_at,p.ewma_output_tps_confidence FROM model_performance p JOIN models m ON m.platform=p.platform AND m.model_id=p.model_id WHERE m.id=?1",[id],|r|Ok((r.get::<_,Option<f64>>(0)?,r.get::<_,Option<f64>>(1)?,r.get::<_,Option<f64>>(2)?,r.get::<_,i64>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<f64>>(5)?))).optional()?;
    if let Some((tps, ttft, latency, samples, date, confidence)) = performance {
        for (key, value, unit) in [
            ("performance.output_tps", tps, "tokens/s"),
            ("performance.ttft_ms", ttft, "ms"),
            ("performance.latency_ms", latency, "ms"),
        ] {
            if let Some(value) = value {
                let fresh = chrono::DateTime::parse_from_rfc3339(&date).is_ok_and(|d| {
                    (chrono::Utc::now() - d.with_timezone(&chrono::Utc)).num_hours() < 24
                });
                rows.push(json!({"id":-(id*10+rows.len() as i64+1),"metric":key,"value":{"type":"number","value":value},"unit":unit,"source":"local-observed","scope":"localObservation","confidence":confidence.unwrap_or(0.0),"fetchedAt":date,"observedAt":date,"benchmarkVersion":"","verified":false,"sampleCount":samples,"freshness":if fresh{"fresh"}else{"stale"},"conflict":false,"selectionReason":"provider-specific EWMA; routing requires at least 5 samples"}));
            }
        }
    }
    Ok(json!(rows))
}

pub fn candidates_for(conn: &Connection, id: i64) -> rusqlite::Result<Value> {
    let canonical: Option<i64> = conn.query_row(
        "SELECT canonical_model_id FROM models WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    let mut stmt=conn.prepare("SELECT o.id,o.metric,o.value_json,o.unit,o.source,o.source_model_id,o.scope,o.fetched_at,o.benchmark_version,o.original_json,o.verified,EXISTS(SELECT 1 FROM model_metadata_effective e WHERE e.observation_id=o.id),o.confidence FROM model_metadata_observations o WHERE o.target_key=?1 OR o.target_key=?2 ORDER BY o.metric,o.source,o.id LIMIT 500")?;
    let values=stmt.query_map(params![format!("d:{id}"),canonical.map(|c|format!("c:{c}")).unwrap_or_default()],|r|{let val:String=r.get(2)?;let original:Option<String>=r.get(9)?;Ok(json!({"id":r.get::<_,i64>(0)?,"metric":r.get::<_,String>(1)?,"value":serde_json::from_str::<Value>(&val).unwrap_or(Value::Null),"unit":r.get::<_,String>(3)?,"source":r.get::<_,String>(4)?,"sourceModelId":r.get::<_,String>(5)?,"scope":r.get::<_,String>(6)?,"fetchedAt":r.get::<_,String>(7)?,"benchmarkVersion":r.get::<_,String>(8)?,"original":original.and_then(|v|serde_json::from_str::<Value>(&v).ok()),"verified":r.get::<_,bool>(10)?,"selected":r.get::<_,bool>(11)?,"confidence":r.get::<_,f64>(12)?,"freshness":"candidate","selectionReason":"source candidate; selected=false means another observation takes priority"}))})?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!(values))
}

/// Reconcile existing evidence after discovery, without HTTP or rewriting collection timestamps.
pub fn rematch(conn: &mut Connection) -> rusqlite::Result<()> {
    crate::services::rankings::enrich::ensure_identity_registry(conn)?;
    let index = MatchIndex::load(conn)?;
    let identities = {
        let mut stmt=conn.prepare("SELECT source,source_model_id,model_id,provider,scope FROM model_metadata_observations GROUP BY source,source_model_id,scope")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let tx = conn.transaction()?;
    for (source, key, model, provider, scope) in identities {
        let metric_scope = if scope == "canonicalModel" {
            MetricScope::CanonicalModel
        } else {
            MetricScope::ProviderDeployment
        };
        let matched =
            index.resolve_identity(&source, &key, &model, provider.as_deref(), metric_scope);
        let target = if let Some(d) = matched.deployment {
            format!("d:{d}")
        } else if metric_scope == MetricScope::CanonicalModel {
            matched
                .canonical
                .map(|c| format!("c:{c}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        tx.execute("UPDATE model_metadata_observations SET canonical_model_id=?1,deployment_id=?2,target_key=?3,match_status=?4 WHERE source=?5 AND source_model_id=?6 AND scope=?7",params![matched.canonical,matched.deployment,target,matched.status,source,key,scope])?;
    }
    recompute(&tx)?;
    tx.commit()
}
