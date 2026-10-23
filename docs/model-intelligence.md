# Model intelligence registry

The Models dashboard (`/models`) separates canonical quality, provider deployment specifications, and local observations. Expand a model to inspect deployments, effective values, all source candidates, and historical changes. Missing capabilities are **Unknown**; an explicit false is **Unsupported**. Comparisons separate metric, source, benchmark version, and scope; missing/incomparable cells are N/A.

## Compatibility boundary and migration

The backend remains Rust/Axum/rusqlite/SQLite. Existing `models.id` and `(platform, model_id)` are deployment identity; no duplicate deployment table is introduced. Existing `canonical_models`, `model_aliases`, `model_benchmarks`, `ranking_source_models`, `ranking_unmatched`, `ranking_source_status`, `model_performance`, API keys, settings, and fallback rows remain intact.

Existing model/ranking APIs retain their DTO keys, sentinel-to-null conversion, quality/speed fields, routing strategy aliases, and manual order. The original ranking refresh remains operational. Model discovery now also refreshes due metadata sources and reconciles cached metadata with newly discovered deployments. Existing ranking fields remain legacy projections; the richer evidence contract is the metadata endpoint. Legacy `pricing_prompt` and `pricing_completion` are **USD/token**; new `price.*` observations are explicitly **USD/1M tokens**. Do not copy these values between the two representations without conversion.

Initialization applies additive, idempotent DDL from `services/model_metadata/schema.sql` after legacy table/column upgrades. Added tables:

- `metadata_sources`: enablement, TTL, conditional cache validators, backoff, health.
- `model_metadata_observations`: typed evidence, source identity, scope, canonical/deployment links, unit, version, original price units, matching state.
- `model_metadata_effective`: selected observation and selection reason per non-null target key/metric.
- `metadata_sync_runs`: source outcome and observation counters.
- `model_metric_history`: changed values only, with source, units, version, timestamp.
- `metadata_match_decisions` and `metadata_match_audit`: reviewed approval/rejection with reasons.
- `model_performance_samples`: bounded local observations for rolling summaries.
- Optional `model_catalog_fts`: FTS5 index with catalog/alias triggers; LIKE fallback when FTS5 is unavailable.

Existing manual mappings are never reassigned by identity initialization. The matching normalization no longer removes dates, preview/experimental variants, or revision hashes. Existing persisted associations are preserved rather than destructively rewritten. Only an explicit audited administrative resolution can replace a reviewed source alias. Source/model identity plus metric/scope/non-null version forms the natural ingestion key, avoiding SQLite NULL uniqueness pitfalls.

Back up a real installation before upgrading, e.g. `sqlite3 data/freeapi.db '.backup data/freeapi-before-metadata.db'`. Tests use isolated databases, include repeat migration and SQLite backup/reopen, and never touch a user development database. Legacy compatibility tests remain in the workspace suite. No production DB was available or modified during implementation.

## Sources and authorization

| Source | Default | Actual evidence |
|---|---|---|
| Artificial Analysis | Enabled; requires `ARTIFICIAL_ANALYSIS_API_KEY` | Free endpoint's intelligence/coding/agentic and named capability indices, source UUID/slug, numeric index version, model-wide median TPS, TTFT/end-to-end latency when present, input/output/cache prices, release/publisher |
| Models.dev | Enabled; public | Provider/model IDs, canonical hints for review, family/name/description, context/input/output limits, modalities, explicit tools/reasoning/schema support, release, open-weight flag, input/output/cache prices |
| OpenRouter | Disabled until enabled in Sources | Exact catalog variants including `:free`, supported parameters, modalities, context/max output, per-token prices converted to per-million, explicit zero-token-price evidence |
| LiteLLM | Disabled until enabled in Sources | Provider-scoped cost/specification fallback, explicit capability booleans, limits, per-token and cache prices converted to per-million |
| Hugging Face Hub | Disabled and requires explicit repo list | Public repo ID, model type, card license, safetensors total parameters when exposed; canonical identity evidence only |

For Hugging Face set `HUGGINGFACE_METADATA_REPOS=owner/model,other/model` (at most 20 validated repository IDs) and enable the source. No Hub-wide scans, gated/private access bypass, or inference probes occur. Missing model-card fields are absent evidence, not guesses. A Hub repository is not a hosting deployment.

Artificial Analysis retains `ARTIFICIAL_ANALYSIS_URL` and the existing Free default. Metadata overrides accept only the official HTTPS language-model endpoint paths on `artificialanalysis.ai`, without credentials/query strings. Other catalog URLs are fixed. Redirects are rejected; payloads, pagination, observations, connection times and request times are bounded. Errors expose status categories, never upstream bodies, API keys, or credential-bearing URLs.

The Free AA response does not guarantee Pro fields. No context window, quantization, provider performance, or dedicated reasoning score is fabricated. Global median performance remains canonical evidence; it never becomes observed provider performance. The optional `best-reasoning` strategy stays unscored without a dedicated comparable reasoning metric and falls back deterministically. LiveBench is deliberately not registered: an authorized versioned machine-readable adapter can implement `ModelMetadataSource`; website scraping and blending unrelated raw scores are not implemented.

Source references inspected:

- https://artificialanalysis.ai/data-api/docs/reference
- https://models.dev/ and https://models.dev/api.json
- https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties
- https://docs.litellm.ai/docs/proxy/model_cost_map
- https://github.com/BerriAI/litellm/blob/main/model_prices_and_context_window.json

Keep attribution visible. Models.dev and LiteLLM data retain their MIT attribution; AA/OpenRouter/Hub usage remains subject to each service's API terms and model-specific licenses.

## Refresh and selection

`ModelMetadataSource` provides typed observations through a configurable registry. Fetching/parsing occurs before acquiring a SQLite guard. At most two sources refresh concurrently. Each has an overlap guard, TTL, ETag/Last-Modified where supplied, Retry-After support and capped exponential backoff with jitter. Manual refresh bypasses TTL, not active backoff or source disablement. Missing credentials do not accumulate exponential backoff. Paginated responses do not reuse a first-page validator as an aggregate cache validator.

Failures retain last-good observations. Missing optional fields do not clear prior evidence; explicit false updates do. The effective-value resolver orders verified evidence, freshness, field/source authority, collection time, confidence, and stable identity. Provider catalog prices remain deployment-scoped; AA prices remain canonical. LiteLLM is a secondary fallback. Reviewed legacy manual benchmark overrides retain authority and their own provenance.

File-backed installations write catalog transactions through a separate WAL connection on Tokio's blocking pool, without holding the global SQLite mutex. In-memory tests use the existing connection. Atomic source ingestion retains all validated candidates and updates effective selection in the same transaction. Discovery rematches cached evidence without advancing its collection time or requiring a refetch.

History defaults to 90 days, configurable through the existing `metadata_history_retention_days` setting (clamped to 1–365); at most 100,000 changes are retained. At most 1,000 source run summaries and 100,000 local samples are retained. Identical successful refreshes do not append history. Observations are upserted by identity, not appended on every refresh.

## Administrative API

Every route below uses the existing `/api` admin bearer authentication, including read-only routes. `/v1` authentication remains unchanged. There are no public credential/status exceptions.

| Method/path | Contract |
|---|---|
| `GET /api/metadata/explorer` | Canonical groups with distinct deployments and effective metrics. `query`, `provider`, `capability`, `free`, `minContext`, `sort`, `limit`, `offset`. FTS prefix search includes aliases; fallback uses bound LIKE parameters. |
| `GET /api/models/search` | Existing `{data,meta}` response; adds `query` alias, capability and minimum-context filters. Limit is now clamped to 1–200 and offset to 0–1,000,000. |
| `GET /api/models/{id}/metadata` | `{id,metrics,candidates,performance,windows}`; no provider credentials. |
| `GET /api/models/{id}/metrics` | Same detail DTO. |
| `GET /api/models/{id}/history` | `metric`, `window` in days (1–90), `limit`, `offset`; `{data,limit,offset}`. |
| `GET /api/models/compare?ids=1,2` | 2–4 positive deployment IDs; `{data:[{id,metrics}]}`. |
| `GET /api/metadata/definitions` | Scope and benchmark-comparison semantics. |
| `GET /api/metadata/sources` | Enabled/configured state, domains, TTL, health, last success/failure, attribution, license. No API keys or configurable raw URLs. |
| `PATCH /api/metadata/sources/{id}` | `{enabled?:boolean,ttlSeconds?:number}`; TTL 300–604800. Unknown fields/sources rejected. Recomputes effective evidence. |
| `POST /api/metadata/sync` | Refresh enabled sources; `{sources:[{source,status,fetched,valid,matched,unmatched,ambiguous,changed,durationMs,error}]}`. Counts are observation counts. |
| `GET /api/metadata/sync/status` | Latest 100 completed source runs; Sources also exposes current refreshing state. |
| `GET /api/metadata/matches` | `status` is unmatched/ambiguous/matched/rejected/canonicalOnly; paginated source identities. Canonical hints are unverified suggestions. |
| `POST /api/metadata/matches/{id}/resolve` | `{canonicalModelId:number,reason:string}` or `{reject:true,reason:string}`. Required reason; rejected decisions survive resync. Recomputes cached evidence without HTTP. |
| `GET /api/metadata/matches/{id}/audit` | Last 100 decisions, previous/new canonical IDs and reasons. |
| `GET/PATCH /api/routing/policy` | Read/replace validated administrative routing constraints and weights. No tokens or credentials stored in this policy. |
| `POST /api/routing/simulate` | Validated policy plus input/output token estimates; selected eligible model and per-candidate reasons, score, estimated deployment cost, source evidence. No inference calls. |

Effective fields include typed `value`, `unit`, `source`, `scope`, `confidence`, collection/claimed timestamps, freshness, version, verified state, selection reason and conflict indicator. Detail also exposes candidate values and original price units. Local metrics carry sample count. Read pagination is bounded, and all value filters use parameters or fixed SQL allowlists. Deployments per expanded group and raw candidates are capped to keep individual responses bounded.

## Routing and telemetry

Existing strategies and aliases continue to parse; additive strategies are `lowest-latency`, `cheapest`, `best-coding`, `best-reasoning`, `free-only`, and `weighted`. Before scoring, shared live/simulator eligibility checks provider/key availability, enabled model/fallback, saved provider restrictions, explicit deployment capabilities, context/output, strict free/cost policy, cooldown and current quotas. Unknown/stale capabilities do not satisfy hard requirements. Text/image/audio/video request requirements and JSON-schema/tools requirements are inferred from the actual request and combined with saved policy. Saving a policy applies it to live fallback, not just the simulator.

Quality and speed use stable scales; sparse evidence cannot gain a perfect score by cohort-relative min/max normalization. Weighted scores require at least two substantive measurements and do not redistribute missing evidence to measured fields. Local performance requires at least five samples and freshness within 24 hours for ranking; Beta(8,2) reliability avoids one-success certainty. Price uses known deployment prices and input/output estimates; missing prices reject maximum-cost policies. Saved restrictions cannot be loosened by simulation inputs.

Decode TPS is recorded only with actual generation duration or valid `total latency - TTFT`; non-streaming total latency alone no longer masquerades as decode speed. Missing timing/usage produces no TPS. EWMA remains intact. Rolling 15m/1h/24h summaries expose sample counts and p50/p95 only with at least five timings. Local EWMA is never overwritten by published canonical speed.

Simulation is stateless: explicit model requests, sticky sessions, per-attempt skipped keys and concurrent quota changes can alter the actual eligible fallback order after the displayed snapshot. Hard policies still apply before sticky preference. Existing retries, stream handling, key rotation and provider invocation remain in the original router/proxy path; no new automatic inference costs or retry loops are introduced.

## Validation and limitations

Run offline checks:

```sh
cargo fmt --all -- --check
GETMODELS_NETWORK_TESTS=0 cargo test --workspace --locked -- --skip check_all_with_keys_returns_success
cargo clippy --workspace --all-targets --locked -- -D warnings
npm run build:client
npm run lint --workspace client
```

Fixture and integration coverage includes all catalog shapes, optional/false fields, pricing conversions, AA version/latency, HF scope, exact variants, aliases and rejected decisions, repeat ingestion, changed history, authority, FTS mutation and fallback, backup/reopen, authentication and bounds, cost/context/capability constraints, stale evidence and missing decode timings. Local HTTP tests cover unauthorized, 429/Retry-After, 304/ETag, malformed JSON, timeout and redirects.

Explicit opt-in smoke checks (catalog reads only):

```sh
cargo test -p server --test metadata live_models_dev_schema_smoke -- --ignored --nocapture
METADATA_CATALOG_FIXTURE_PATH=/path/to/downloaded-api.json cargo test -p server --test metadata downloaded_models_dev_schema_smoke -- --ignored --nocapture
METADATA_CATALOG_FIXTURE_PATH=/path/to/downloaded-api.json cargo test -p server --test metadata downloaded_catalog_ingestion_smoke -- --ignored --nocapture
```

The downloaded current Models.dev catalog was checked: 159,938 validated observations from 232 providers. The isolated initial ingestion took about 7.6 seconds in a debug build. File-backed background writes therefore use the blocking WAL writer; large writes can still contend with other SQLite writers. The direct live reqwest smoke failed with a transport error in this execution environment; it is not reported as a passed live refresh. No AA credential was available, so AA Free parsing is verified with fixtures and official documentation, not an authenticated live response.

Strict Clippy on Rust 1.99.1 currently reports two pre-existing `result_large_err` warnings in authentication middleware. New-code Clippy was also run with only that existing lint allowed. An unfiltered workspace run was stopped by automatic approval review because the existing `check_all_with_keys_returns_success` test calls the live Google API. The offline suite explicitly excludes that one test; it is not claimed as passed. The final offline workspace run passed 337 tests, with four existing/opt-in tests ignored. Full frontend ESLint reports six pre-existing Fast Refresh export errors in shared UI components; changed frontend files pass targeted ESLint. Client typecheck/production build pass. Browser validation was attempted but Chromium downloads returned truncated archives, so mobile visual/accessibility behavior has not been browser-verified. CI billing failures are not used as code-quality evidence.

The optional LiveBench adapter remains unregistered. Dedicated reasoning measurements, provider-specific AA Pro/Commercial performance, gated Hub metadata, synthetic probes, automatic fuzzy merges, and exhaustive benchmark-suite normalization are not claimed. New adapters must provide authorized machine-readable evidence, stable metric/version definitions, fixtures and attribution before being registered.
