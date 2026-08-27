//! Common types shared by ranking sources and the enrichment engine.
//!
//! A missing metric is represented as `None` all the way through the source
//! and persistence layers. No source is allowed to turn an unknown value into
//! a score, rank, or zero.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceBenchmark {
    pub intelligence_score: Option<f64>,
    pub speed_tokens_per_sec: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalBenchmark {
    /// Stable source identifier. For Artificial Analysis this is the UUID;
    /// it is retained as metadata and is not the local identity key.
    pub source_id: String,
    pub source_slug: Option<String>,
    pub intelligence_score: Option<f64>,
    pub speed_tokens_per_sec: Option<f64>,
    pub fetched_at: DateTime<Utc>,
    pub raw_updated_at: Option<String>,
}

impl ExternalBenchmark {
    pub fn as_source_benchmark(&self) -> SourceBenchmark {
        SourceBenchmark {
            intelligence_score: self.intelligence_score,
            speed_tokens_per_sec: self.speed_tokens_per_sec,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RankingError {
    NotConfigured(String),
    Http(String),
    InvalidResponse(String),
}

impl std::fmt::Display for RankingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured(message) | Self::Http(message) | Self::InvalidResponse(message) => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for RankingError {}

#[async_trait]
pub trait RankingSourceProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn fetch(&self) -> Result<Vec<ExternalBenchmark>, RankingError>;
}

#[derive(Clone, Copy, Debug)]
pub struct RankingSource {
    pub name: &'static str,
}
