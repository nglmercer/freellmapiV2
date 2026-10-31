//! Integration fixtures contain synthetic values, never production benchmark defaults.
mod common;
use serde_json::{json, Value};
use server::{
    db::db,
    services::model_metadata::{
        persistence,
        routing::{self, RoutingPolicy},
        sources,
        types::MetricValue,
    },
};
fn fixture(id: &str) -> Vec<server::services::model_metadata::types::MetadataObservation> {
    let text = match id {
        "models-dev" => include_str!("fixtures/metadata/models-dev.json"),
        "artificial-analysis" => include_str!("fixtures/metadata/artificial-analysis.json"),
        "openrouter" => include_str!("fixtures/metadata/openrouter.json"),
        _ => include_str!("fixtures/metadata/litellm.json"),
    };
    sources::parse(id, &serde_json::from_str(text).unwrap()).unwrap()
}
async fn model_id() -> i64 {
    db().lock()
        .await
        .query_row(
            "SELECT id FROM models WHERE platform='google' AND model_id='gemini-2.5-pro'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
#[tokio::test]
async fn fixture_ingestion_is_idempotent_and_preserves_partial_evidence() {
    let _app = common::setup().await;
    let id = model_id().await;
    let mut conn = db().lock().await;
    let rows = fixture("models-dev");
    let first = persistence::ingest(&mut conn, "models-dev", &rows).unwrap();
    assert_eq!(first.matched, rows.len());
    assert!(first.changed > 0);
    assert_eq!(
        persistence::ingest(&mut conn, "models-dev", &rows)
            .unwrap()
            .changed,
        0
    );
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM model_metadata_observations",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, rows.len() as i64);
    let history: i64 = conn
        .query_row("SELECT COUNT(*) FROM model_metric_history", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(history, rows.len() as i64);
    let partial = sources::parse(
        "models-dev",
        &json!({"google":{"models":{"gemini-2.5-pro":{"tool_call":false}}}}),
    )
    .unwrap();
    persistence::ingest(&mut conn, "models-dev", &partial).unwrap();
    let metrics = persistence::metrics_for(&conn, id).unwrap();
    assert!(metrics
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["metric"] == "context.total"));
    assert!(metrics
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["metric"] == "capability.tools" && r["value"]["value"] == false));
}
#[tokio::test]
async fn source_precedence_and_disabling_recompute_without_network() {
    let _app = common::setup().await;
    let id = model_id().await;
    let mut conn = db().lock().await;
    conn.execute(
        "UPDATE metadata_sources SET enabled=1 WHERE id='litellm'",
        [],
    )
    .unwrap();
    persistence::ingest(&mut conn, "models-dev", &fixture("models-dev")).unwrap();
    persistence::ingest(&mut conn, "litellm", &fixture("litellm")).unwrap();
    let values = persistence::metrics_for(&conn, id).unwrap();
    let price = values
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["metric"] == "price.input")
        .unwrap();
    assert_eq!(price["source"], "models-dev");
    assert_eq!(price["conflict"], true);
    conn.execute(
        "UPDATE metadata_sources SET enabled=0 WHERE id='models-dev'",
        [],
    )
    .unwrap();
    persistence::recompute(&conn).unwrap();
    let values = persistence::metrics_for(&conn, id).unwrap();
    assert!(values
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["metric"] == "price.input"
            && r["source"] == "litellm"
            && r["value"]["value"] == 3.0));
}
#[tokio::test]
async fn corrupt_numeric_values_do_not_replace_known_good() {
    let _app = common::setup().await;
    let mut conn = db().lock().await;
    let rows = fixture("models-dev");
    persistence::ingest(&mut conn, "models-dev", &rows).unwrap();
    let mut bad = rows
        .iter()
        .find(|o| o.metric == "price.input")
        .unwrap()
        .clone();
    bad.value = MetricValue::Number(f64::NAN);
    let r = persistence::ingest(&mut conn, "models-dev", &[bad.clone()]).unwrap();
    assert_eq!(r.valid, 0);
    bad.value = MetricValue::Number(-1.0);
    assert_eq!(
        persistence::ingest(&mut conn, "models-dev", &[bad])
            .unwrap()
            .valid,
        0
    );
}
#[tokio::test]
async fn aa_preserves_version_units_and_does_not_invent_pro_fields() {
    let _app = common::setup().await;
    let rows = fixture("artificial-analysis");
    assert!(rows
        .iter()
        .any(|r| r.metric == "performance.ttft_ms" && r.value == MetricValue::Number(650.0)));
    assert!(rows.iter().any(|r| r.metric == "benchmark.aa.coding"));
    assert!(!rows.iter().any(|r| r.metric == "benchmark.aa.agentic"));
    assert!(rows
        .iter()
        .all(|r| r.source_model_id == "fixture-aa-uuid"
            && r.benchmark_version.as_deref() == Some("4.1")));
    assert!(!rows.iter().any(|r| r.provider.is_some()));
}
#[tokio::test]
async fn reviewed_aliases_survive_source_refresh_and_index_updates() {
    let app = common::setup().await;
    let rows = fixture("artificial-analysis");
    let id = model_id().await;
    {
        let mut conn = db().lock().await;
        persistence::ingest(&mut conn, "artificial-analysis", &rows).unwrap();
    }
    let (canonical, observation) = {
        let conn = db().lock().await;
        (conn.query_row("SELECT canonical_model_id FROM models WHERE id=?1",[id],|r|r.get::<_,i64>(0)).unwrap(),conn.query_row("SELECT MIN(id) FROM model_metadata_observations WHERE source='artificial-analysis'",[],|r|r.get::<_,i64>(0)).unwrap())
    };
    let response = common::post_json(
        &app.app,
        &format!("/api/metadata/matches/{observation}/resolve"),
        &app.api_key,
        json!({"canonicalModelId":canonical,"reason":"Reviewed exact upstream identity"}),
    )
    .await;
    assert_eq!(response.status, 200, "{}", response.text);
    let mut conn = db().lock().await;
    persistence::ingest(&mut conn, "artificial-analysis", &rows).unwrap();
    let alias:(i64,bool)=conn.query_row("SELECT canonical_model_id,verified FROM model_aliases WHERE source='artificial-analysis' AND source_model_id='fixture-aa-uuid'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(alias, (canonical, true));
    let audits: i64 = conn
        .query_row("SELECT COUNT(*) FROM metadata_match_audit", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(audits, 1);
}
#[tokio::test]
async fn rejected_match_does_not_return_on_next_sync() {
    let app = common::setup().await;
    {
        let mut c = db().lock().await;
        persistence::ingest(
            &mut c,
            "artificial-analysis",
            &fixture("artificial-analysis"),
        )
        .unwrap();
    }
    let id: i64 = db()
        .lock()
        .await
        .query_row("SELECT MIN(id) FROM model_metadata_observations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let response = common::post_json(
        &app.app,
        &format!("/api/metadata/matches/{id}/resolve"),
        &app.api_key,
        json!({"reject":true,"reason":"Different variant"}),
    )
    .await;
    assert_eq!(response.status, 200);
    let mut c = db().lock().await;
    persistence::ingest(
        &mut c,
        "artificial-analysis",
        &fixture("artificial-analysis"),
    )
    .unwrap();
    let status: String = c
        .query_row(
            "SELECT match_status FROM model_metadata_observations WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "rejected");
}
#[tokio::test]
async fn api_is_admin_only_validates_and_bounds_queries() {
    let app = common::setup().await;
    for path in [
        "/api/metadata/sources",
        "/api/metadata/explorer",
        "/api/metadata/matches",
    ] {
        let r = common::request(&app.app, "GET", path, None, None).await;
        assert_eq!(r.status, 401);
    }
    assert_eq!(
        common::get(&app.app, "/api/metadata/explorer?limit=-100&offset=-5")
            .await
            .body["limit"],
        1
    );
    assert_eq!(
        common::get(&app.app, "/api/models/search?limit=-100&offset=-5")
            .await
            .body["meta"]["limit"],
        1
    );
    assert_eq!(
        common::get(&app.app, "/api/models/compare?ids=1,2,3,4,5")
            .await
            .status,
        400
    );
    assert_eq!(
        common::get(&app.app, "/api/models/999999/metadata")
            .await
            .status,
        404
    );
    assert_eq!(
        common::get(&app.app, "/api/metadata/explorer?capability=not-real")
            .await
            .status,
        400
    );
    let r = common::request(
        &app.app,
        "PATCH",
        "/api/metadata/sources/models-dev",
        Some(&common::admin_key()),
        Some(json!({"ttlSeconds":-1})),
    )
    .await;
    assert_eq!(r.status, 400);
}
#[tokio::test]
async fn metadata_api_search_compare_and_history_return_provenance() {
    let app = common::setup().await;
    let id = model_id().await;
    {
        let mut c = db().lock().await;
        persistence::ingest(&mut c, "models-dev", &fixture("models-dev")).unwrap();
    }
    let response = common::get(&app.app, &format!("/api/models/{id}/metadata")).await;
    assert_eq!(response.status, 200);
    assert!(response.body["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["source"] == "models-dev" && m["scope"] == "providerDeployment"));
    let response = common::get(
        &app.app,
        "/api/metadata/explorer?capability=tools&query=Gemini",
    )
    .await;
    assert_eq!(response.status, 200, "{}", response.text);
    assert!(response.body["total"].as_i64().unwrap() > 0);
    let history = common::get(
        &app.app,
        &format!("/api/models/{id}/history?metric=price.input"),
    )
    .await;
    assert_eq!(history.status, 200);
    assert_eq!(history.body["data"][0]["unit"], "USD/1M tokens");
}
#[tokio::test]
async fn schema_reopen_and_repeat_preserve_keys_priorities_and_evidence() {
    let _app = common::setup().await;
    let mut conn = db().lock().await;
    persistence::ingest(&mut conn, "models-dev", &fixture("models-dev")).unwrap();
    conn.execute(
        "INSERT INTO settings(key,value) VALUES('test-preserved-key','secret-placeholder')",
        [],
    )
    .unwrap();
    conn.execute("UPDATE fallback_config SET manual_priority=137 WHERE id=(SELECT MIN(id) FROM fallback_config)",[]).unwrap();
    persistence::migrate(&conn).unwrap();
    persistence::migrate(&conn).unwrap();
    let file = std::env::temp_dir().join(format!("metadata-backup-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&file);
    conn.execute("VACUUM INTO ?1", [file.to_str().unwrap()])
        .unwrap();
    let reopened = rusqlite::Connection::open(&file).unwrap();
    persistence::migrate(&reopened).unwrap();
    assert_eq!(
        reopened
            .query_row(
                "SELECT value FROM settings WHERE key='test-preserved-key'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "secret-placeholder"
    );
    assert_eq!(
        reopened
            .query_row(
                "SELECT manual_priority FROM fallback_config ORDER BY id LIMIT 1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        137
    );
    assert_eq!(
        reopened
            .query_row(
                "SELECT COUNT(*) FROM model_metadata_observations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        fixture("models-dev").len() as i64
    );
    drop(reopened);
    std::fs::remove_file(file).unwrap();
}
#[tokio::test]
async fn routing_rejects_unknown_caps_cost_context_and_paid_fallback() {
    let _app = common::setup().await;
    let id = model_id().await;
    let mut conn = db().lock().await;
    persistence::ingest(&mut conn, "models-dev", &fixture("models-dev")).unwrap();
    let p = RoutingPolicy {
        required_capabilities: vec!["structured_output".into()],
        free_only: true,
        max_cost_usd: Some(0.0),
        input_tokens: 2_000_000,
        ..Default::default()
    };
    let d = routing::evaluate(&conn, id, &p).unwrap();
    assert!(!d.eligible);
    assert!(d.rejection_reasons.contains(&"freeOnly".to_string()));
    assert!(d
        .rejection_reasons
        .iter()
        .any(|s| s.contains("structured_output")));
    assert!(d
        .rejection_reasons
        .contains(&"insufficientOrUnknownContext".to_string()));
    assert!(d
        .rejection_reasons
        .contains(&"costLimitOrUnknownPrice".to_string()));
    let p = RoutingPolicy {
        input_tokens: 1000,
        output_tokens: 500,
        ..Default::default()
    };
    let d = routing::evaluate(&conn, id, &p).unwrap();
    assert_eq!(d.estimated_cost_usd, Some(0.00625));
    assert!(d.score.is_none());
}
#[tokio::test]
async fn telemetry_without_decode_timing_does_not_fabricate_tps() {
    let _app = common::setup().await;
    use server::services::telemetry::*;
    record_observation(
        "google",
        "gemini-2.5-pro",
        RuntimeObservation {
            outcome: ObservationOutcome::Success,
            latency_ms: Some(2000),
            ttft_ms: None,
            output_tokens: Some(100),
            generation_duration_ms: None,
        },
    )
    .await
    .unwrap();
    let conn = db().lock().await;
    let snapshot = load_performance(&conn, "google", "gemini-2.5-pro")
        .unwrap()
        .unwrap();
    assert!(snapshot.ewma_output_tps.is_none());
    let windows = rolling_summaries(&conn, "google", "gemini-2.5-pro").unwrap();
    assert_eq!(windows[0]["sampleCount"], 1);
    assert!(windows[0]["p95LatencyMs"].is_null());
}
#[test]
fn variants_are_never_collapsed_by_matching() {
    use server::services::rankings::match_ids::canonicalize;
    for (a, b) in [
        ("model-preview", "model"),
        ("model-2026-10-01", "model"),
        ("model@abc123def", "model"),
        ("model-thinking", "model-instruct"),
        ("model-q4", "model-q8"),
    ] {
        assert_ne!(canonicalize(a), canonicalize(b));
    }
}
#[test]
fn all_adapters_support_empty_partial_and_invalid_shapes() {
    for (source, data) in [
        ("models-dev", json!({})),
        ("litellm", json!({})),
        ("openrouter", json!({"data":[]})),
        ("artificial-analysis", json!({"data":[]})),
    ] {
        assert!(sources::parse(source, &data).unwrap().is_empty());
        assert!(sources::parse(source, &Value::Null).is_err());
    }
    let rows = fixture("openrouter");
    assert!(rows
        .iter()
        .any(|r| r.metric == "availability.free" && r.value == MetricValue::Boolean(true)));
    assert!(rows.iter().all(|r| r.model_id.ends_with(":free")));
}

#[test]
fn hf_is_opt_in_identity_evidence_never_deployment_availability() {
    use server::services::model_metadata::types::MetricScope;
    let rows=sources::parse("huggingface",&json!({"id":"example/model-preview","cardData":{"license":"apache-2.0"},"config":{"model_type":"llama"},"safetensors":{"total":7000000000_u64}})).unwrap();
    assert!(rows
        .iter()
        .all(|o| o.scope == MetricScope::CanonicalModel && o.provider.is_none()));
    assert!(rows.iter().any(|o| o.metric == "identity.parameters"));
    assert!(!rows.iter().any(|o| o.metric.starts_with("availability.")));
    assert!(!sources::safe_url(
        "huggingface",
        "https://huggingface.co/api/models/../../secrets"
    ));
    assert!(sources::safe_url(
        "huggingface",
        "https://huggingface.co/api/models/example/model-preview"
    ));
}
#[tokio::test]
async fn failed_source_state_keeps_last_good_and_marks_old_evidence_stale() {
    let _app = common::setup().await;
    let id = model_id().await;
    let mut conn = db().lock().await;
    persistence::ingest(&mut conn, "models-dev", &fixture("models-dev")).unwrap();
    conn.execute("UPDATE metadata_sources SET status='rateLimited',last_error='rateLimited' WHERE id='models-dev'",[]).unwrap();
    conn.execute("UPDATE model_metadata_observations SET fetched_at='2020-01-01T00:00:00Z' WHERE source='models-dev'",[]).unwrap();
    persistence::recompute(&conn).unwrap();
    let metrics = persistence::metrics_for(&conn, id).unwrap();
    assert!(!metrics.as_array().unwrap().is_empty());
    assert!(metrics
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["freshness"] == "stale"));
}
#[tokio::test]
async fn fts_alias_changes_and_like_fallback_are_maintained() {
    let app = common::setup().await;
    let id = model_id().await;
    {
        let mut conn = db().lock().await;
        persistence::ingest(&mut conn, "models-dev", &fixture("models-dev")).unwrap();
        let canonical: i64 = conn
            .query_row(
                "SELECT canonical_model_id FROM models WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute("INSERT INTO model_aliases(canonical_model_id,source,source_model_id,verified) VALUES(?1,'manual','uniquesearchalias',1)",[canonical]).unwrap();
    }
    let r = common::get(&app.app, "/api/metadata/explorer?query=uniquesearchalias").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.body["total"], 1);
    {
        let conn = db().lock().await;
        conn.execute("UPDATE model_aliases SET source_model_id='replacedsearchalias' WHERE source_model_id='uniquesearchalias'",[]).unwrap();
    }
    assert_eq!(
        common::get(&app.app, "/api/metadata/explorer?query=uniquesearchalias")
            .await
            .body["total"],
        0
    );
    {
        let conn = db().lock().await;
        conn.execute_batch("DROP TRIGGER metadata_models_insert;DROP TRIGGER metadata_models_update;DROP TRIGGER metadata_models_delete;DROP TRIGGER metadata_alias_insert;DROP TRIGGER metadata_alias_update;DROP TRIGGER metadata_alias_delete;DROP TABLE model_catalog_fts;").unwrap();
    }
    assert_eq!(
        common::get(&app.app, "/api/metadata/explorer?query=replacedsearchalias")
            .await
            .body["total"],
        1
    );
}

#[tokio::test]
#[ignore = "explicit opt-in live catalog fetch; no inference requests"]
async fn live_models_dev_schema_smoke() {
    use server::services::model_metadata::{
        sources::CatalogSource,
        types::{CacheHeaders, ModelMetadataSource},
    };
    let fetched = CatalogSource("models-dev")
        .fetch(CacheHeaders::default())
        .await
        .expect("public catalog fetch");
    let rows = fetched.observations.unwrap();
    assert!(!rows.is_empty());
    let valid = rows
        .iter()
        .filter(|o| server::services::model_metadata::metrics::validate(o))
        .count();
    assert!(valid > 100);
    let providers: std::collections::HashSet<_> =
        rows.iter().filter_map(|o| o.provider.as_deref()).collect();
    assert!(providers.len() > 1);
    println!(
        "Live Models.dev: {} observations, {} valid, {} providers",
        rows.len(),
        valid,
        providers.len()
    );
}

#[tokio::test]
async fn cached_metadata_matches_new_deployments_without_refetch() {
    let _app = common::setup().await;
    let rows = sources::parse(
        "models-dev",
        &json!({"groq":{"models":{"new-model-preview":{"tool_call":true}}}}),
    )
    .unwrap();
    let mut conn = db().lock().await;
    let result = persistence::ingest(&mut conn, "models-dev", &rows).unwrap();
    assert_eq!(result.matched, 0);
    let original: String = conn
        .query_row(
            "SELECT fetched_at FROM model_metadata_observations LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute("INSERT INTO models(platform,model_id,display_name,intelligence_rank,speed_rank) VALUES('groq','new-model-preview','New',99,99)",[]).unwrap();
    persistence::rematch(&mut conn).unwrap();
    let (status, date): (String, String) = conn
        .query_row(
            "SELECT match_status,fetched_at FROM model_metadata_observations LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "matched");
    assert_eq!(date, original);
}
#[test]
#[ignore = "explicit schema check against a separately downloaded public catalog"]
fn downloaded_models_dev_schema_smoke() {
    let path =
        std::env::var("METADATA_CATALOG_FIXTURE_PATH").expect("set METADATA_CATALOG_FIXTURE_PATH");
    let body = std::fs::read_to_string(path).unwrap();
    let data: Value = serde_json::from_str(&body).unwrap();
    let rows = sources::parse("models-dev", &data).unwrap();
    let valid = rows
        .iter()
        .filter(|o| server::services::model_metadata::metrics::validate(o))
        .count();
    assert!(valid > 100);
    let providers: std::collections::HashSet<_> =
        rows.iter().filter_map(|o| o.provider.as_deref()).collect();
    assert!(providers.len() > 1);
    println!(
        "Downloaded Models.dev: {} observations, {} valid, {} providers",
        rows.len(),
        valid,
        providers.len()
    );
}

#[tokio::test]
#[ignore = "explicit integration performance check with a downloaded public catalog"]
async fn downloaded_catalog_ingestion_smoke() {
    let _app = common::setup().await;
    let path =
        std::env::var("METADATA_CATALOG_FIXTURE_PATH").expect("set METADATA_CATALOG_FIXTURE_PATH");
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rows = sources::parse("models-dev", &data).unwrap();
    let mut conn = db().lock().await;
    let start = std::time::Instant::now();
    let result = persistence::ingest(&mut conn, "models-dev", &rows).unwrap();
    assert_eq!(result.valid, rows.len());
    println!(
        "Ingested {} observations in {:?}; {} matched",
        result.valid,
        start.elapsed(),
        result.matched
    );
}

#[tokio::test]
async fn simulator_and_live_selection_share_cost_policy_and_never_fall_back_to_paid() {
    // Selection only: no provider method or health check is invoked.
    let app = common::setup().await;
    let policy = RoutingPolicy {
        strategy: Some("cheapest".into()),
        input_tokens: 1000,
        output_tokens: 500,
        free_only: true,
        ..Default::default()
    };
    let expected = {
        let mut conn = db().lock().await;
        conn.execute(
            "UPDATE metadata_sources SET enabled=1 WHERE id='openrouter'",
            [],
        )
        .unwrap();
        persistence::ingest(&mut conn, "models-dev", &fixture("models-dev")).unwrap();
        persistence::ingest(&mut conn, "openrouter", &fixture("openrouter")).unwrap();
        for platform in ["google", "openrouter"] {
            let (e, iv, tag) = server::crypto::encrypt("offline-test-key");
            conn.execute("INSERT INTO api_keys(platform,label,encrypted_key,iv,auth_tag,status,enabled) VALUES(?1,'offline selection only',?2,?3,?4,'healthy',1)",rusqlite::params![platform,e,iv,tag]).unwrap();
        }
        let selected =
            server::services::router::route_request_with_policy(&conn, 1500, None, None, &policy)
                .unwrap();
        assert_eq!(selected.platform, "openrouter");
        selected.model_db_id
    };
    let simulated = common::post_json(
        &app.app,
        "/api/routing/simulate",
        &app.api_key,
        json!(policy),
    )
    .await;
    assert_eq!(simulated.status, 200);
    assert_eq!(simulated.body["selectedModelId"], expected);
    let conn = db().lock().await;
    conn.execute(
        "UPDATE fallback_config SET enabled=0 WHERE model_db_id=?1",
        [expected],
    )
    .unwrap();
    assert!(
        server::services::router::route_request_with_policy(&conn, 1500, None, None, &policy)
            .is_err()
    );
}
#[tokio::test]
async fn saved_policy_validates_and_enforces_shared_constraints() {
    let app = common::setup().await;
    let invalid = common::request(
        &app.app,
        "PATCH",
        "/api/routing/policy",
        Some(&common::admin_key()),
        Some(json!({"weights":{"quality":-1}})),
    )
    .await;
    assert_eq!(invalid.status, 400);
    let response=common::request(&app.app,"PATCH","/api/routing/policy",Some(&common::admin_key()),Some(json!({"strategy":"weighted","freeOnly":true,"maxCostUsd":0,"requiredCapabilities":["vision"]}))).await;
    assert_eq!(response.status, 200);
    let conn = db().lock().await;
    let request = RoutingPolicy {
        max_cost_usd: Some(100.0),
        ..Default::default()
    };
    let merged = routing::configured_policy(&conn, &request);
    assert!(merged.free_only);
    assert_eq!(merged.max_cost_usd, Some(0.0));
    assert_eq!(merged.required_capabilities, vec!["vision"]);
}

#[tokio::test]
async fn case_collisions_are_reviewable_instead_of_last_row_wins() {
    let _app = common::setup().await;
    let mut conn = db().lock().await;
    for model in ["case-model", "CASE-MODEL"] {
        conn.execute("INSERT INTO models(platform,model_id,display_name,intelligence_rank,speed_rank) VALUES('groq',?1,?1,99,99)",[model]).unwrap();
    }
    let rows = sources::parse(
        "models-dev",
        &json!({"groq":{"models":{"case-model":{"tool_call":true}}}}),
    )
    .unwrap();
    let result = persistence::ingest(&mut conn, "models-dev", &rows).unwrap();
    assert_eq!(result.ambiguous, 1);
    assert_eq!(result.matched, 0);
}
