//! Port of `server/src/services/rankings/sources/artificial-analysis.ts`.

use serde::Deserialize;

use crate::providers::base::{http_client, send_request};
use crate::services::rankings::match_ids::{build_rank_index, RankLookup};
use crate::services::rankings::types::{RankingSource, SourceBenchmark};

const DEFAULT_URL: &str = "https://artificialanalysis.ai/api/v2/models";
const TIMEOUT_MS: u64 = 15_000;

#[derive(Deserialize)]
struct AARawModel {
    id: Option<String>,
    name: Option<String>,
    slug: Option<String>,
    model_id: Option<String>,
    intelligence_index: Option<f64>,
    intelligence: Option<f64>,
    output_tokens_per_second: Option<f64>,
    speed_tokens_per_second: Option<f64>,
    output_speed: Option<f64>,
    speed: Option<f64>,
    #[allow(dead_code)]
    context_window: Option<f64>,
}

#[derive(Deserialize)]
struct AAResponse {
    data: Option<Vec<AARawModel>>,
    models: Option<Vec<AARawModel>>,
}

#[derive(Clone)]
struct AABenchmark {
    /// The model id as published by AA, used to build the lookup index.
    id: String,
    benchmark: SourceBenchmark,
}

fn pick_id(m: &AARawModel) -> Option<&str> {
    m.id.as_deref().or(m.slug.as_deref()).or(m.model_id.as_deref()).or(m.name.as_deref())
}

fn pick_intelligence(m: &AARawModel) -> Option<f64> {
    m.intelligence_index
        .or(m.intelligence)
        .filter(|v| v.is_finite())
}

fn pick_speed(m: &AARawModel) -> Option<f64> {
    m.output_tokens_per_second
        .or(m.speed_tokens_per_second)
        .or(m.output_speed)
        .or(m.speed)
        .filter(|v| v.is_finite())
}

async fn fetch_aa_benchmarks() -> Result<Vec<AABenchmark>, String> {
    let url = crate::env::env_string("ARTIFICIAL_ANALYSIS_URL")
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let mut req = http_client()
        .get(&url)
        .header("User-Agent", "freellmapi/1.0")
        .header("Accept", "application/json");
    if let Some(api_key) = crate::env::env_string("ARTIFICIAL_ANALYSIS_API_KEY") {
        req = req.header("AA-API-Key", &api_key);
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = send_request(req, Some(TIMEOUT_MS))
        .await
        .map_err(|e| e.message)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("AA responded {} {}", status.as_u16(), status.canonical_reason().unwrap_or("")));
    }
    let body: AAResponse = resp.json().await.map_err(|e| e.to_string())?;
    let list = body.data.or(body.models).unwrap_or_default();

    let mut out = Vec::new();
    for m in &list {
        let (Some(id), Some(intel), Some(speed)) = (pick_id(m), pick_intelligence(m), pick_speed(m))
        else {
            continue;
        };
        out.push(AABenchmark {
            id: id.to_string(),
            benchmark: SourceBenchmark {
                intelligence_score: intel,
                speed_tokens_per_sec: speed,
            },
        });
    }
    Ok(out)
}

pub const ARTIFICIAL_ANALYSIS_SOURCE: RankingSource = RankingSource {
    name: "artificial-analysis",
};

pub async fn fetch_aa_lookup() -> Option<RankLookup> {
    match fetch_aa_benchmarks().await {
        Ok(benchmarks) => Some(build_rank_index(
            benchmarks
                .into_iter()
                .map(|b| (b.id, b.benchmark))
                .collect::<Vec<_>>(),
        )),
        Err(err) => {
            tracing::warn!("[rankings] artificial-analysis fetch failed: {err}");
            None
        }
    }
}
