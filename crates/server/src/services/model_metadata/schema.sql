CREATE TABLE IF NOT EXISTS metadata_sources (
 id TEXT PRIMARY KEY, enabled INTEGER NOT NULL, ttl_seconds INTEGER NOT NULL CHECK(ttl_seconds BETWEEN 300 AND 604800),
 status TEXT NOT NULL DEFAULT 'never', last_success TEXT, last_failure TEXT, last_error TEXT,
 failures INTEGER NOT NULL DEFAULT 0, next_attempt INTEGER NOT NULL DEFAULT 0, etag TEXT, modified TEXT
);
CREATE TABLE IF NOT EXISTS model_metadata_observations (
 id INTEGER PRIMARY KEY, source TEXT NOT NULL REFERENCES metadata_sources(id), source_model_id TEXT NOT NULL,
 model_id TEXT NOT NULL, provider TEXT, canonical_hint TEXT,
 canonical_model_id INTEGER REFERENCES canonical_models(id), deployment_id INTEGER REFERENCES models(id) ON DELETE CASCADE,
 target_key TEXT NOT NULL, metric TEXT NOT NULL, scope TEXT NOT NULL, value_json TEXT NOT NULL,
 unit TEXT NOT NULL, fetched_at TEXT NOT NULL, observed_at TEXT, benchmark_version TEXT NOT NULL DEFAULT '',
 confidence REAL NOT NULL CHECK(confidence BETWEEN 0 AND 1), verified INTEGER NOT NULL DEFAULT 0,
 original_json TEXT, match_status TEXT NOT NULL,
 UNIQUE(source, source_model_id, scope, metric, benchmark_version)
);
CREATE INDEX IF NOT EXISTS idx_metadata_target ON model_metadata_observations(target_key, metric);
CREATE INDEX IF NOT EXISTS idx_metadata_canonical ON model_metadata_observations(canonical_model_id,metric);
CREATE INDEX IF NOT EXISTS idx_metadata_deployment ON model_metadata_observations(deployment_id,metric);
CREATE INDEX IF NOT EXISTS idx_metadata_source_time ON model_metadata_observations(source,fetched_at);
CREATE INDEX IF NOT EXISTS idx_metadata_matches ON model_metadata_observations(match_status, source, source_model_id);
CREATE TABLE IF NOT EXISTS model_metadata_effective (
 target_key TEXT NOT NULL, metric TEXT NOT NULL, observation_id INTEGER NOT NULL REFERENCES model_metadata_observations(id) ON DELETE CASCADE,
 selection_reason TEXT NOT NULL, PRIMARY KEY(target_key, metric)
);
CREATE TABLE IF NOT EXISTS metadata_sync_runs (
 id INTEGER PRIMARY KEY, source TEXT NOT NULL, started_at TEXT NOT NULL, completed_at TEXT NOT NULL,
 status TEXT NOT NULL, summary_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_metadata_runs ON metadata_sync_runs(source, id DESC);
CREATE TABLE IF NOT EXISTS model_metric_history (
 id INTEGER PRIMARY KEY, target_key TEXT NOT NULL, source TEXT NOT NULL, metric TEXT NOT NULL,
 value_json TEXT NOT NULL, unit TEXT NOT NULL, benchmark_version TEXT NOT NULL, changed_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_metric_history ON model_metric_history(target_key,metric,changed_at);
CREATE TABLE IF NOT EXISTS metadata_match_decisions (
 source TEXT NOT NULL, source_model_id TEXT NOT NULL, decision TEXT NOT NULL,
 canonical_model_id INTEGER REFERENCES canonical_models(id), reason TEXT NOT NULL,
 updated_at TEXT NOT NULL, PRIMARY KEY(source,source_model_id)
);
CREATE TABLE IF NOT EXISTS metadata_match_audit (
 id INTEGER PRIMARY KEY, source TEXT NOT NULL, source_model_id TEXT NOT NULL, decision TEXT NOT NULL,
 previous_canonical_id INTEGER, canonical_model_id INTEGER, reason TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS model_performance_samples (
 id INTEGER PRIMARY KEY, platform TEXT NOT NULL, model_id TEXT NOT NULL, success INTEGER NOT NULL,
 latency_ms REAL, ttft_ms REAL, output_tps REAL, output_tokens INTEGER, created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_performance_samples ON model_performance_samples(platform,model_id,created_at);
