//! Artificial Analysis Data API integration.
//!
//! The documented language-model endpoint returns an envelope whose model
//! metrics are nested under `evaluations` and `performance`. The parser is
//! intentionally fixture-testable and does not scrape the website HTML.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::providers::base::{http_client, send_request};
use crate::services::rankings::match_ids::{build_rank_index, RankLookup};
use crate::services::rankings::types::{
    ExternalBenchmark, RankingError, RankingSource, RankingSourceProvider,
};

const DEFAULT_URL: &str = "https://artificialanalysis.ai/api/v2/language/models/free";
const TIMEOUT_MS: u64 = 15_000;
const MAX_PAGES: usize = 100;

#[derive(Debug, Deserialize, Default)]
struct AAEvaluations {
    artificial_analysis_intelligence_index: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct AAPerformance {
    median_output_tokens_per_second: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct AAModel {
    id: Option<String>,
    slug: Option<String>,
    name: Option<String>,
    #[serde(alias = "updatedAt")]
    updated_at: Option<String>,
    #[serde(alias = "lastUpdatedAt")]
    last_updated_at: Option<String>,
    evaluations: Option<AAEvaluations>,
    performance: Option<AAPerformance>,
    // `/api/v2/data/llms/models` is a documented compatible shape. It is
    // accepted as a fallback for compatible deployments.
    median_output_tokens_per_second: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct AAPagination {
    page: Option<usize>,
    total_pages: Option<usize>,
    has_more: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct AAResponse {
    data: Option<Vec<AAModel>>,
    pagination: Option<AAPagination>,
}

fn finite_metric(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn parse_page(
    body: &str,
    fetched_at: DateTime<Utc>,
) -> Result<(Vec<ExternalBenchmark>, bool), RankingError> {
    let response: AAResponse = serde_json::from_str(body).map_err(|error| {
        RankingError::InvalidResponse(format!("Artificial Analysis JSON: {error}"))
    })?;
    let pagination = response.pagination.unwrap_or_default();
    let has_more = pagination.has_more.unwrap_or_else(|| {
        pagination
            .total_pages
            .zip(pagination.page)
            .is_some_and(|(total, page)| page < total)
    });

    let data = response.data.ok_or_else(|| {
        RankingError::InvalidResponse("Artificial Analysis response is missing data".to_string())
    })?;
    let benchmarks =
        data.into_iter()
            .filter_map(|model| {
                let source_slug = model.slug.or_else(|| model.name.clone());
                let source_id = model.id.or_else(|| source_slug.clone())?;
                let intelligence_score =
                    finite_metric(model.evaluations.and_then(|evaluations| {
                        evaluations.artificial_analysis_intelligence_index
                    }));
                let speed_tokens_per_sec = finite_metric(
                    model
                        .performance
                        .and_then(|performance| performance.median_output_tokens_per_second)
                        .or(model.median_output_tokens_per_second),
                );
                Some(ExternalBenchmark {
                    source_id,
                    source_slug,
                    intelligence_score,
                    speed_tokens_per_sec,
                    fetched_at,
                    raw_updated_at: model.updated_at.or(model.last_updated_at),
                })
            })
            .collect();

    Ok((benchmarks, has_more))
}

/// Parse a current AA response without making a network request.
pub fn parse_aa_response(
    body: &str,
    fetched_at: DateTime<Utc>,
) -> Result<Vec<ExternalBenchmark>, RankingError> {
    Ok(parse_page(body, fetched_at)?.0)
}

fn endpoint_url(configured: Option<String>) -> String {
    let configured = configured.unwrap_or_else(|| DEFAULT_URL.to_string());
    let trimmed = configured.trim_end_matches('/');
    if trimmed.ends_with("/language/models")
        || trimmed.ends_with("/language/models/free")
        || trimmed.ends_with("/data/llms/models")
        || trimmed.ends_with("/models")
        || trimmed.ends_with("/models/free")
    {
        trimmed.to_string()
    } else {
        format!("{trimmed}/language/models")
    }
}

fn page_url(url: &str, page: usize) -> String {
    if page == 1 {
        url.to_string()
    } else if url.contains('?') {
        format!("{url}&page={page}")
    } else {
        format!("{url}?page={page}")
    }
}

#[derive(Clone, Debug)]
pub struct ArtificialAnalysisProvider {
    url: String,
    api_key: Option<String>,
}

impl ArtificialAnalysisProvider {
    pub fn from_env() -> Self {
        Self {
            url: endpoint_url(crate::env::env_string("ARTIFICIAL_ANALYSIS_URL")),
            api_key: crate::env::env_string("ARTIFICIAL_ANALYSIS_API_KEY"),
        }
    }

    pub fn new(url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            url: endpoint_url(Some(url.into())),
            api_key,
        }
    }
}

#[async_trait::async_trait]
impl RankingSourceProvider for ArtificialAnalysisProvider {
    fn name(&self) -> &'static str {
        "artificial-analysis"
    }

    async fn fetch(&self) -> Result<Vec<ExternalBenchmark>, RankingError> {
        let Some(api_key) = self.api_key.as_deref().filter(|key| !key.trim().is_empty()) else {
            return Err(RankingError::NotConfigured(
                "ARTIFICIAL_ANALYSIS_API_KEY is not configured".to_string(),
            ));
        };

        let fetched_at = Utc::now();
        let mut page = 1;
        let mut all = Vec::new();
        loop {
            let req = http_client()
                .get(page_url(&self.url, page))
                .header("Accept", "application/json")
                .header("x-api-key", api_key);
            let response = send_request(req, Some(TIMEOUT_MS))
                .await
                .map_err(|error| RankingError::Http(error.message))?;
            let status = response.status();
            if !status.is_success() {
                return Err(RankingError::Http(format!(
                    "Artificial Analysis responded {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("")
                )));
            }
            let body = response
                .text()
                .await
                .map_err(|error| RankingError::Http(error.to_string()))?;
            let (mut benchmarks, has_more) = parse_page(&body, fetched_at)?;
            all.append(&mut benchmarks);
            if !has_more || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }
        Ok(all)
    }
}

pub const ARTIFICIAL_ANALYSIS_SOURCE: RankingSource = RankingSource {
    name: "artificial-analysis",
};

/// Compatibility helper for older internal callers. New code should consume
/// `ExternalBenchmark` values so it can retain source UUID/slug metadata.
pub async fn fetch_aa_lookup() -> Option<RankLookup> {
    let provider = ArtificialAnalysisProvider::from_env();
    match provider.fetch().await {
        Ok(benchmarks) => Some(build_rank_index(
            benchmarks
                .into_iter()
                .map(|benchmark| {
                    (
                        benchmark
                            .source_slug
                            .clone()
                            .unwrap_or_else(|| benchmark.source_id.clone()),
                        benchmark.as_source_benchmark(),
                    )
                })
                .collect::<Vec<_>>(),
        )),
        Err(error) => {
            tracing::warn!("[rankings] artificial-analysis fetch failed: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetched_at() -> DateTime<Utc> {
        "2026-01-01T00:00:00Z".parse().unwrap()
    }

    #[test]
    fn parses_checked_in_fixture_with_partial_metrics() {
        let body = include_str!("../../../../tests/fixtures/artificial-analysis-models.json");
        let models = parse_aa_response(body, fetched_at()).unwrap();
        assert_eq!(models.len(), 4);
        assert_eq!(
            models[0].raw_updated_at.as_deref(),
            Some("2026-08-26T12:00:00Z")
        );
        assert_eq!(models[1].intelligence_score, Some(52.8));
        assert_eq!(models[1].speed_tokens_per_sec, None);
        assert_eq!(models[2].intelligence_score, None);
        assert_eq!(models[2].speed_tokens_per_sec, Some(210.0));
        assert_eq!(models[3].intelligence_score, None);
        assert_eq!(models[3].speed_tokens_per_sec, None);
    }

    #[test]
    fn parses_nested_metrics_and_retains_uuid() {
        let body = r#"{
          "pagination": {"page": 1, "total_pages": 1, "has_more": false},
          "data": [{
            "id": "36f73aaf-d38a-4b56-a2b3-d04d17186910",
            "slug": "gpt-4o",
            "evaluations": {"artificial_analysis_intelligence_index": 55.8},
            "performance": {"median_output_tokens_per_second": 143.2}
          }]
        }"#;
        let models = parse_aa_response(body, fetched_at()).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].source_id, "36f73aaf-d38a-4b56-a2b3-d04d17186910");
        assert_eq!(models[0].source_slug.as_deref(), Some("gpt-4o"));
        assert_eq!(models[0].intelligence_score, Some(55.8));
        assert_eq!(models[0].speed_tokens_per_sec, Some(143.2));
    }

    #[test]
    fn parses_partial_and_empty_metrics() {
        let body = r#"{"data":[
          {"id":"quality-only","slug":"quality-only","evaluations":{"artificial_analysis_intelligence_index":52.8}},
          {"id":"speed-only","slug":"speed-only","performance":{"median_output_tokens_per_second":210}},
          {"id":"empty","slug":"empty"}
        ]}"#;
        let models = parse_aa_response(body, fetched_at()).unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].speed_tokens_per_sec, None);
        assert_eq!(models[1].intelligence_score, None);
        assert_eq!(models[2].intelligence_score, None);
    }

    #[test]
    fn malformed_response_is_an_error() {
        assert!(parse_aa_response("not json", fetched_at()).is_err());
        assert!(parse_aa_response("{}", fetched_at()).is_err());
    }

    #[test]
    fn pagination_metadata_is_honored() {
        let (_, has_more) = parse_page(
            r#"{"data":[],"pagination":{"page":1,"total_pages":2}}"#,
            fetched_at(),
        )
        .unwrap();
        assert!(has_more);
    }

    #[test]
    fn free_endpoint_is_the_default_but_can_be_overridden() {
        assert_eq!(
            endpoint_url(None),
            "https://artificialanalysis.ai/api/v2/language/models/free"
        );
        assert_eq!(
            endpoint_url(Some(
                "https://artificialanalysis.ai/api/v2/language/models".to_string()
            )),
            "https://artificialanalysis.ai/api/v2/language/models"
        );
    }
}
