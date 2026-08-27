//! Types for external ranking sources.
//!
//! All sources must return real benchmark data fetched from their API —
//! never inferred from model name substrings, version tags, or pricing tiers.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SourceBenchmark {
    /// raw score as reported by the source (e.g. AA intelligence_index)
    pub intelligence_score: f64,
    /// tokens/sec output speed as reported by the source
    pub speed_tokens_per_sec: f64,
}

pub struct RankingSource {
    /// short identifier, stored in models.ranking_source
    pub name: &'static str,
}
