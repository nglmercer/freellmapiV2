//! Shared bounded HTTP transport and catalog adapters. No source can set arbitrary URLs.
use super::metrics::{definition, per_million};
use super::types::*;
use chrono::Utc;
use futures::StreamExt;
use serde_json::{json, Value};
use std::{sync::OnceLock, time::Duration};
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_OBSERVATIONS: usize = 500_000;
#[derive(Clone)]
pub struct CatalogSource(pub &'static str);
pub fn registry() -> Vec<Box<dyn ModelMetadataSource>> {
    [
        "artificial-analysis",
        "models-dev",
        "openrouter",
        "litellm",
        "huggingface",
    ]
    .into_iter()
    .map(|id| Box::new(CatalogSource(id)) as Box<dyn ModelMetadataSource>)
    .collect()
}
pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("freellmapi/0.1 metadata")
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()
            .expect("metadata HTTP client")
    })
}
fn valid_hf_repo(repo: &str) -> bool {
    let parts: Vec<_> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 100
                && *part != "."
                && *part != ".."
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
}
fn hf_repos() -> Vec<String> {
    crate::env::env_string("HUGGINGFACE_METADATA_REPOS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|r| valid_hf_repo(r))
        .take(20)
        .map(str::to_owned)
        .collect()
}
pub fn safe_url(id: &str, url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    let host = match id {
        "artificial-analysis" => "artificialanalysis.ai",
        "models-dev" => "models.dev",
        "openrouter" => "openrouter.ai",
        "litellm" => "raw.githubusercontent.com",
        "huggingface" => "huggingface.co",
        _ => return false,
    };
    u.scheme() == "https"
        && u.host_str() == Some(host)
        && u.port_or_known_default() == Some(443)
        && u.username().is_empty()
        && u.password().is_none()
        && u.fragment().is_none()
        && u.query().is_none()
        && match id {
            "artificial-analysis" => matches!(
                u.path(),
                "/api/v2/language/models/free"
                    | "/api/v2/language/models"
                    | "/api/v2/data/llms/models"
            ),
            "models-dev" => u.path() == "/api.json",
            "openrouter" => u.path() == "/api/v1/models",
            "huggingface" => u
                .path()
                .strip_prefix("/api/models/")
                .is_some_and(valid_hf_repo),
            "litellm" => u.path() == "/BerriAI/litellm/main/model_prices_and_context_window.json",
            _ => false,
        }
}
#[async_trait::async_trait]
impl ModelMetadataSource for CatalogSource {
    fn definition(&self) -> SourceDefinition {
        let (url, attribution, license, domains, ttl, enabled): (&str, &str, &str, &[&str], i64, bool) = match self.0 {
            "artificial-analysis" => ("https://artificialanalysis.ai/api/v2/language/models/free", "Artificial Analysis", "Artificial Analysis API terms; attribution required", &["benchmarks", "performance", "pricing"], 86400, true),
            "models-dev" => ("https://models.dev/api.json", "Models.dev / Anomaly", "MIT", &["catalog", "capabilities", "context", "pricing"], 86400, true),
            "openrouter" => ("https://openrouter.ai/api/v1/models", "OpenRouter", "OpenRouter API terms", &["deployments", "capabilities", "context", "pricing"], 21600, false),
            "huggingface" => ("https://huggingface.co/api/models/", "Hugging Face Hub", "Hub API terms; model-specific license", &["identity", "license", "parameters"], 86400, false),
            _ => ("https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json", "LiteLLM / BerriAI", "MIT, copyright Berri AI", &["catalog", "capabilities", "pricing"], 86400, false),
        };
        SourceDefinition {
            id: self.0,
            url: if self.0 == "artificial-analysis" {
                crate::env::env_string("ARTIFICIAL_ANALYSIS_URL").unwrap_or_else(|| url.into())
            } else {
                url.into()
            },
            attribution,
            license,
            domains,
            ttl_seconds: ttl,
            default_enabled: enabled,
            configured: if self.0 == "huggingface" {
                !hf_repos().is_empty()
            } else {
                self.0 != "artificial-analysis"
                    || crate::env::env_string("ARTIFICIAL_ANALYSIS_API_KEY")
                        .is_some_and(|s| !s.trim().is_empty())
            },
        }
    }
    async fn fetch(&self, cache: CacheHeaders) -> Result<FetchResult, MetadataError> {
        let def = self.definition();
        if !def.configured {
            return Err(MetadataError::new("notConfigured"));
        }
        if self.0 == "huggingface" {
            let mut observations = Vec::new();
            for repo in hf_repos() {
                let mut target = def.clone();
                target.url = format!("https://huggingface.co/api/models/{repo}");
                if !safe_url(target.id, &target.url) {
                    return Err(MetadataError::new("invalidUrl"));
                }
                let fetched = fetch_from(self, CacheHeaders::default(), target, client()).await?;
                observations.extend(fetched.observations.unwrap_or_default());
            }
            return Ok(FetchResult {
                observations: Some(observations),
                cache: CacheHeaders::default(),
            });
        }
        if !safe_url(def.id, &def.url) {
            return Err(MetadataError::new("invalidUrl"));
        }
        fetch_from(self, cache, def, client()).await
    }
}
// Private transport entry for deterministic local HTTP tests; production always validates safe_url.
async fn fetch_from(
    source: &CatalogSource,
    cache: CacheHeaders,
    def: SourceDefinition,
    http: &reqwest::Client,
) -> Result<FetchResult, MetadataError> {
    let mut all = Vec::new();
    let mut headers = CacheHeaders::default();
    for page in 1..=100 {
        let mut req = http.get(&def.url).header("Accept", "application/json");
        if source.0 == "artificial-analysis" {
            req = req.header(
                "x-api-key",
                crate::env::env_string("ARTIFICIAL_ANALYSIS_API_KEY").unwrap_or_default(),
            );
            if page > 1 {
                req = req.query(&[("page", page)]);
            }
        }
        if page == 1 {
            if let Some(v) = &cache.etag {
                req = req.header("If-None-Match", v);
            }
            if let Some(v) = &cache.modified {
                req = req.header("If-Modified-Since", v);
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| MetadataError::new(if e.is_timeout() { "timeout" } else { "failed" }))?;
        let status = resp.status().as_u16();
        if status == 304 && page == 1 {
            if cache.etag.is_none() && cache.modified.is_none() {
                return Err(MetadataError::new("malformed"));
            }
            return Ok(FetchResult {
                observations: None,
                cache,
            });
        }
        if !(200..300).contains(&status) {
            return Err(MetadataError {
                status: match status {
                    401 | 403 => "unauthorized",
                    429 => "rateLimited",
                    _ => "failed",
                },
                retry_after: resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| {
                        v.parse::<i64>().ok().or_else(|| {
                            chrono::DateTime::parse_from_rfc2822(v)
                                .ok()
                                .map(|d| (d.with_timezone(&Utc) - Utc::now()).num_seconds())
                        })
                    })
                    .map(|s| s.clamp(1, 86400)),
            });
        }
        if page == 1 {
            headers.etag = resp
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            headers.modified = resp
                .headers()
                .get("last-modified")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
        }
        let data = read_document(resp).await?;
        let mut observations = parse(source.0, &data)?;
        if all.len() + observations.len() > MAX_OBSERVATIONS {
            return Err(MetadataError::new("payloadTooLarge"));
        }
        all.append(&mut observations);
        let more = data
            .pointer("/pagination/has_more")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                data.pointer("/pagination/total_pages")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n > page)
            });
        if source.0 != "artificial-analysis" || !more {
            let mut seen = std::collections::HashSet::new();
            if all.iter().any(|o| {
                !seen.insert((
                    o.source_model_id.clone(),
                    o.metric.clone(),
                    o.benchmark_version.clone(),
                ))
            }) {
                return Err(MetadataError::new("duplicateIdentity"));
            }
            return Ok(FetchResult {
                observations: Some(all),
                cache: if page > 1 {
                    CacheHeaders::default()
                } else {
                    headers
                },
            });
        }
    }
    Err(MetadataError::new("paginationLimit"))
}

pub(crate) async fn read_document(resp: reqwest::Response) -> Result<Value, MetadataError> {
    if resp.content_length().is_some_and(|n| n > MAX_BYTES as u64) {
        return Err(MetadataError::new("payloadTooLarge"));
    }
    let mut bytes = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| MetadataError::new("failed"))?;
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(MetadataError::new("payloadTooLarge"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let data: Value =
        serde_json::from_slice(&bytes).map_err(|_| MetadataError::new("malformed"))?;
    Ok(data)
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str()?.parse().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
}
struct Builder {
    source: String,
    key: String,
    model: String,
    provider: Option<String>,
    hint: Option<String>,
    observed: Option<String>,
    version: Option<String>,
    scope: MetricScope,
    rows: Vec<MetadataObservation>,
}
impl Builder {
    fn add(&mut self, key: &str, value: MetricValue, original: Option<Value>) {
        let Some(d) = definition(key) else {
            return;
        };
        self.rows.push(MetadataObservation {
            source: self.source.clone(),
            source_model_id: self.key.clone(),
            model_id: self.model.clone(),
            provider: self.provider.clone(),
            canonical_hint: self.hint.clone(),
            metric: key.into(),
            value,
            unit: d.unit.into(),
            scope: self.scope,
            fetched_at: Utc::now(),
            observed_at: self.observed.clone(),
            benchmark_version: self.version.clone(),
            confidence: if self.source == "litellm" { 0.7 } else { 0.9 },
            verified: false,
            original_value: original,
        });
    }
    fn fields(&mut self, model: &Value, fields: &[(&str, &str, &str)]) {
        for (path, key, kind) in fields {
            let Some(v) = model.pointer(path) else {
                continue;
            };
            let val = match *kind {
                "number" => number(v).map(MetricValue::Number),
                "price" => number(v).and_then(per_million).map(MetricValue::Number),
                "bool" => v.as_bool().map(MetricValue::Boolean),
                "json" => v.as_array().map(|_| MetricValue::Json(v.clone())),
                _ => v
                    .as_str()
                    .map(|s| MetricValue::Text(s.chars().take(8192).collect())),
            };
            if let Some(val) = val {
                self.add(
                    key,
                    val,
                    (*kind == "price").then(|| json!({"value":v,"unit":"USD/token"})),
                );
            }
        }
    }
    fn modalities(&mut self, model: &Value, path: &str) {
        if let Some(inputs) = model.pointer(path).and_then(Value::as_array) {
            for (modality, key) in [
                ("image", "capability.vision"),
                ("audio", "capability.audio_input"),
                ("video", "capability.video_input"),
            ] {
                self.add(
                    key,
                    MetricValue::Boolean(inputs.iter().any(|s| s.as_str() == Some(modality))),
                    None,
                );
            }
        }
    }
}
pub fn parse(source: &str, data: &Value) -> Result<Vec<MetadataObservation>, MetadataError> {
    let mut result = Vec::new();
    let mut add = |provider: Option<&str>, id: &str, m: &Value| -> Result<(), MetadataError> {
        if !m.is_object() || id.is_empty() {
            return Err(MetadataError::new("malformed"));
        }
        let mut b = Builder {
            source: source.into(),
            key: provider
                .map(|p| format!("{p}/{id}"))
                .unwrap_or_else(|| id.into()),
            model: id.into(),
            provider: provider.map(str::to_owned),
            hint: m
                .get("canonical_model_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            observed: m
                .get("last_updated")
                .or_else(|| m.get("updated_at"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            version: data
                .get("intelligence_index_version")
                .or_else(|| data.get("benchmark_version"))
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                }),
            scope: if matches!(source, "artificial-analysis" | "huggingface") {
                MetricScope::CanonicalModel
            } else {
                MetricScope::ProviderDeployment
            },
            rows: Vec::new(),
        };
        b.fields(
            m,
            &[
                ("/name", "identity.name", "text"),
                ("/description", "identity.description", "text"),
                ("/family", "identity.family", "text"),
                ("/release_date", "identity.release_date", "text"),
                ("/open_weights", "identity.open_weights", "bool"),
            ],
        );
        match source {
            "huggingface" => {
                b.fields(
                    m,
                    &[
                        ("/id", "identity.huggingface", "text"),
                        ("/cardData/license", "identity.license", "text"),
                        ("/config/model_type", "identity.architecture", "text"),
                        ("/safetensors/total", "identity.parameters", "number"),
                    ],
                );
            }
            "models-dev" => {
                b.fields(
                    m,
                    &[
                        ("/limit/context", "context.total", "number"),
                        ("/limit/input", "context.input", "number"),
                        ("/limit/output", "context.output", "number"),
                        ("/cost/input", "price.input", "number"),
                        ("/cost/output", "price.output", "number"),
                        ("/cost/cache_read", "price.cached_input", "number"),
                        ("/tool_call", "capability.tools", "bool"),
                        ("/reasoning", "capability.reasoning", "bool"),
                        ("/structured_output", "capability.structured_output", "bool"),
                        ("/modalities/input", "modalities.input", "json"),
                        ("/modalities/output", "modalities.output", "json"),
                    ],
                );
                b.modalities(m, "/modalities/input");
            }
            "openrouter" => {
                b.fields(
                    m,
                    &[
                        ("/context_length", "context.total", "number"),
                        (
                            "/top_provider/max_completion_tokens",
                            "context.output",
                            "number",
                        ),
                        ("/pricing/prompt", "price.input", "price"),
                        ("/pricing/completion", "price.output", "price"),
                        ("/pricing/input_cache_read", "price.cached_input", "price"),
                        ("/architecture/input_modalities", "modalities.input", "json"),
                        (
                            "/architecture/output_modalities",
                            "modalities.output",
                            "json",
                        ),
                        ("/supported_parameters", "parameters.supported", "json"),
                    ],
                );
                b.modalities(m, "/architecture/input_modalities");
                if let Some(params) = m.get("supported_parameters").and_then(Value::as_array) {
                    for (param, metric) in [
                        ("tools", "capability.tools"),
                        ("structured_outputs", "capability.structured_output"),
                        ("reasoning", "capability.reasoning"),
                    ] {
                        b.add(
                            metric,
                            MetricValue::Boolean(params.iter().any(|p| p.as_str() == Some(param))),
                            None,
                        );
                    }
                }
                if let (Some(i), Some(o)) = (
                    m.pointer("/pricing/prompt").and_then(number),
                    m.pointer("/pricing/completion").and_then(number),
                ) {
                    b.add(
                        "availability.free",
                        MetricValue::Boolean(i == 0.0 && o == 0.0),
                        None,
                    );
                }
            }
            "litellm" => b.fields(
                m,
                &[
                    ("/max_input_tokens", "context.input", "number"),
                    ("/max_output_tokens", "context.output", "number"),
                    ("/input_cost_per_token", "price.input", "price"),
                    ("/output_cost_per_token", "price.output", "price"),
                    (
                        "/cache_read_input_token_cost",
                        "price.cached_input",
                        "price",
                    ),
                    ("/supports_function_calling", "capability.tools", "bool"),
                    (
                        "/supports_response_schema",
                        "capability.structured_output",
                        "bool",
                    ),
                    ("/supports_reasoning", "capability.reasoning", "bool"),
                    ("/supports_vision", "capability.vision", "bool"),
                ],
            ),
            "artificial-analysis" => {
                // Keep the legacy validated parser as the authority for basic metrics/identity.
                let legacy =
                    crate::services::rankings::sources::artificial_analysis::parse_aa_response(
                        &json!({"data":[m]}).to_string(),
                        Utc::now(),
                    )
                    .map_err(|_| MetadataError::new("malformed"))?;
                if let Some(old) = legacy.first() {
                    b.key = old.source_id.clone();
                    b.model = old
                        .source_slug
                        .clone()
                        .unwrap_or_else(|| old.source_id.clone());
                    b.observed = old.raw_updated_at.clone();
                    if let Some(n) = old.intelligence_score {
                        b.add("benchmark.aa.intelligence", MetricValue::Number(n), None);
                    }
                    if let Some(n) = old.speed_tokens_per_sec {
                        b.add("performance.output_tps", MetricValue::Number(n), None);
                    }
                }
                b.fields(
                    m,
                    &[
                        (
                            "/evaluations/artificial_analysis_coding_index",
                            "benchmark.aa.coding",
                            "number",
                        ),
                        (
                            "/evaluations/artificial_analysis_agentic_index",
                            "benchmark.aa.agentic",
                            "number",
                        ),
                        (
                            "/performance/median_time_to_first_token_seconds",
                            "performance.ttft_ms",
                            "number",
                        ),
                        (
                            "/performance/median_end_to_end_response_time_seconds",
                            "performance.latency_ms",
                            "number",
                        ),
                        (
                            "/pricing/price_1m_cache_hit_tokens",
                            "price.cached_input",
                            "number",
                        ),
                        ("/model_creator/name", "identity.publisher", "text"),
                        ("/pricing/price_1m_input_tokens", "price.input", "number"),
                        ("/pricing/price_1m_output_tokens", "price.output", "number"),
                    ],
                );
                if let Some(evaluations) = m.get("evaluations").and_then(Value::as_object) {
                    for (key, v) in evaluations {
                        if key.starts_with("artificial_analysis_")
                            && ![
                                "artificial_analysis_intelligence_index",
                                "artificial_analysis_coding_index",
                                "artificial_analysis_agentic_index",
                            ]
                            .contains(&key.as_str())
                        {
                            if let Some(n) = number(v) {
                                b.add(&format!("benchmark.aa.{key}"), MetricValue::Number(n), None);
                            }
                        }
                    }
                }
                for o in &mut b.rows {
                    if o.metric == "performance.ttft_ms" || o.metric == "performance.latency_ms" {
                        if let MetricValue::Number(n) = &mut o.value {
                            o.original_value = Some(json!({"value":*n,"unit":"seconds"}));
                            *n *= 1000.0;
                        }
                    }
                }
            }
            _ => return Err(MetadataError::new("unknownSource")),
        }
        result.extend(b.rows);
        Ok(())
    };
    match source {
        "huggingface" => {
            let id = data
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| valid_hf_repo(id))
                .ok_or_else(|| MetadataError::new("malformed"))?;
            add(None, id, data)?;
        }
        "models-dev" => {
            for (provider, p) in data
                .as_object()
                .ok_or_else(|| MetadataError::new("malformed"))?
            {
                let models = p
                    .get("models")
                    .and_then(Value::as_object)
                    .ok_or_else(|| MetadataError::new("malformed"))?;
                for (id, m) in models {
                    add(Some(provider), id, m)?;
                }
            }
        }
        "litellm" => {
            for (id, m) in data
                .as_object()
                .ok_or_else(|| MetadataError::new("malformed"))?
            {
                if id == "sample_spec" {
                    continue;
                }
                let provider = m.get("litellm_provider").and_then(Value::as_str);
                let model = provider
                    .and_then(|p| id.strip_prefix(&format!("{p}/")))
                    .unwrap_or(id);
                add(provider, model, m)?;
            }
        }
        _ => {
            for m in data
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| MetadataError::new("malformed"))?
            {
                let id = m
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| MetadataError::new("malformed"))?;
                add((source == "openrouter").then_some("openrouter"), id, m)?;
            }
        }
    }
    if result.len() > MAX_OBSERVATIONS {
        return Err(MetadataError::new("payloadTooLarge"));
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_false_and_missing() {
        let rows=parse("models-dev", &json!({"groq":{"models":{"model-preview":{"tool_call":false,"limit":{"context":1000},"cost":{"input":0}}}}})).unwrap();
        assert!(rows
            .iter()
            .any(|o| o.metric == "capability.tools" && o.value == MetricValue::Boolean(false)));
        assert!(!rows.iter().any(|o| o.metric == "capability.reasoning"));
        assert!(rows.iter().all(|o| o.model_id == "model-preview"));
    }
    #[test]
    fn converts_litellm_prices() {
        let rows = parse(
            "litellm",
            &json!({"m":{"litellm_provider":"groq","input_cost_per_token":0.000003}}),
        )
        .unwrap();
        assert_eq!(rows[0].value, MetricValue::Number(3.0));
    }
    #[test]
    fn malformed_is_not_empty_success() {
        assert!(parse("models-dev", &json!({"groq":{}})).is_err());
        assert!(parse("openrouter", &json!({})).is_err());
    }
    #[test]
    fn rejects_ssrf_and_redirect_targets() {
        for url in [
            "http://models.dev/api.json",
            "https://127.0.0.1/api.json",
            "https://models.dev@localhost/api.json",
            "https://models.dev/api.json?token=secret",
        ] {
            assert!(!safe_url("models-dev", url));
        }
        assert!(safe_url("models-dev", "https://models.dev/api.json"));
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use axum::{
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    async fn mock(status: u16, body: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new().route(
            "/",
            get(move |headers: HeaderMap| async move {
                if status == 304 {
                    assert_eq!(headers.get("if-none-match").unwrap(), "fixture-etag");
                }
                (
                    StatusCode::from_u16(status).unwrap(),
                    [("retry-after", "120"), ("etag", "fixture-etag")],
                    body,
                )
                    .into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}/"), handle)
    }
    #[tokio::test]
    async fn handles_http_errors_without_payload_or_url_leaks() {
        for (status, expected) in [
            (401, "unauthorized"),
            (403, "unauthorized"),
            (429, "rateLimited"),
            (500, "failed"),
            (302, "failed"),
        ] {
            let (url, task) = mock(status, "private-api-key-must-not-appear").await;
            let source = CatalogSource("models-dev");
            let mut def = source.definition();
            def.url = url;
            let http = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            let Err(error) = fetch_from(&source, CacheHeaders::default(), def, &http).await else {
                panic!("must reject status");
            };
            assert_eq!(error.status, expected);
            assert!(!error.to_string().contains("private"));
            if status == 429 {
                assert_eq!(error.retry_after, Some(120));
            }
            task.abort();
        }
    }
    #[tokio::test]
    async fn handles_conditional_cache_and_malformed_json() {
        let (url, task) = mock(304, "").await;
        let source = CatalogSource("models-dev");
        let mut def = source.definition();
        def.url = url;
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let result = fetch_from(
            &source,
            CacheHeaders {
                etag: Some("fixture-etag".into()),
                modified: None,
            },
            def,
            &http,
        )
        .await
        .unwrap();
        assert!(result.observations.is_none());
        task.abort();
        let (url, task) = mock(200, "not-json").await;
        let mut def = source.definition();
        def.url = url;
        let Err(error) = fetch_from(&source, CacheHeaders::default(), def, &http).await else {
            panic!("must reject malformed JSON")
        };
        assert_eq!(error.status, "malformed");
        task.abort();
    }
    #[tokio::test]
    async fn timeout_is_reported_without_upstream_body() {
        let app = Router::new().route(
            "/",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                "{}"
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let source = CatalogSource("models-dev");
        let mut def = source.definition();
        def.url = format!("http://{addr}/");
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(10))
            .build()
            .unwrap();
        let Err(error) = fetch_from(&source, CacheHeaders::default(), def, &http).await else {
            panic!("expected timeout")
        };
        assert_eq!(error.status, "timeout");
        task.abort();
    }
}
