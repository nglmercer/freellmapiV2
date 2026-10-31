//! Typed evidence. Absence is unknown, an explicit boolean false is evidence.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum MetricValue {
    Number(f64),
    Boolean(bool),
    Text(String),
    Json(serde_json::Value),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricScope {
    CanonicalModel,
    ProviderDeployment,
    LocalObservation,
}
impl MetricScope {
    pub fn key(self) -> &'static str {
        match self {
            Self::CanonicalModel => "canonicalModel",
            Self::ProviderDeployment => "providerDeployment",
            Self::LocalObservation => "localObservation",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataObservation {
    pub source: String,
    pub source_model_id: String,
    pub model_id: String,
    pub provider: Option<String>,
    pub canonical_hint: Option<String>,
    pub metric: String,
    pub value: MetricValue,
    pub unit: String,
    pub scope: MetricScope,
    pub fetched_at: DateTime<Utc>,
    pub observed_at: Option<String>,
    pub benchmark_version: Option<String>,
    pub confidence: f64,
    pub verified: bool,
    pub original_value: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDefinition {
    pub id: &'static str,
    pub url: String,
    pub attribution: &'static str,
    pub license: &'static str,
    pub domains: &'static [&'static str],
    pub ttl_seconds: i64,
    pub default_enabled: bool,
    pub configured: bool,
}
#[derive(Default, Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub source: String,
    pub status: String,
    pub fetched: usize,
    pub valid: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub ambiguous: usize,
    pub changed: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}
#[derive(Debug)]
pub struct MetadataError {
    pub status: &'static str,
    pub retry_after: Option<i64>,
}
impl MetadataError {
    pub fn new(status: &'static str) -> Self {
        Self {
            status,
            retry_after: None,
        }
    }
}
impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.status)
    }
}
impl std::error::Error for MetadataError {}
#[derive(Default)]
pub struct CacheHeaders {
    pub etag: Option<String>,
    pub modified: Option<String>,
}
pub struct FetchResult {
    pub observations: Option<Vec<MetadataObservation>>,
    pub cache: CacheHeaders,
}
#[async_trait::async_trait]
pub trait ModelMetadataSource: Send + Sync {
    fn definition(&self) -> SourceDefinition;
    async fn fetch(&self, cache: CacheHeaders) -> Result<FetchResult, MetadataError>;
}
