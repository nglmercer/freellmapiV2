//! AIMLAPI scraper, mirroring `getmodelsapi/src/scraper/providers/aimlapi.ts`.

use super::ScraperResult;
use crate::types::{Model, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

/// Mirror of `parseFeatures` — maps raw feature strings to a normalized list.
fn parse_features(features: &[String]) -> Vec<String> {
    let feat: Vec<String> = features.iter().map(|f| f.to_lowercase()).collect();
    let mut result: Vec<String> = Vec::new();
    if feat.iter().any(|f| f.contains("chat") || f.contains("completion")) {
        result.push("chat".into());
    }
    if feat.iter().any(|f| f.contains("vision") || f.contains("image")) {
        result.push("vision".into());
    }
    if feat.iter().any(|f| f.contains("function") || f.contains("tool")) {
        result.push("tools".into());
    }
    if feat.iter().any(|f| f.contains("audio") || f.contains("speech")) {
        result.push("audio".into());
    }
    if result.is_empty() {
        result.push("chat".into());
    }
    result
}

/// Scrape the AIMLAPI public models endpoint.
pub async fn scrape_aimlapi(_config: ProviderConfig) -> ScraperResult {
    let http_config = HttpConfig {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    match get_json("https://api.aimlapi.com/v1/models", &http_config).await {
        Ok(value) => {
            let Some(arr) = value.get("data").and_then(|d| d.as_array()) else {
                return ScraperResult::err("Unexpected AIMLAPI response");
            };

            let mut models = Vec::with_capacity(arr.len());
            for m in arr {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let info = m.get("info");
                let name = info
                    .and_then(|i| i.get("name"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&id)
                    .to_string();
                let developer = info
                    .and_then(|i| i.get("developer"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase())
                    .filter(|s| !s.is_empty());
                let features: Vec<String> = m
                    .get("features")
                    .and_then(|f| f.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let context_window = info
                    .and_then(|i| i.get("contextLength"))
                    .and_then(|v| v.as_i64())
                    .filter(|&n| n != 0)
                    .unwrap_or(4096);
                let url = info
                    .and_then(|i| i.get("url"))
                    .and_then(|v| v.as_str())
                    .or_else(|| info.and_then(|i| i.get("docs_url")).and_then(|v| v.as_str()))
                    .map(|s| s.to_string());
                let description = info
                    .and_then(|i| i.get("description"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(300).collect::<String>())
                    .filter(|s| !s.is_empty());

                models.push(Model {
                    id,
                    name,
                    provider: "aimlapi".into(),
                    gateway: developer,
                    context_window,
                    supported_features: parse_features(&features),
                    pricing: None,
                    url,
                    description,
                    free_tier: None,
                });
            }
            ScraperResult::ok(models)
        }
        Err(e) => ScraperResult::err(e.to_string()),
    }
}