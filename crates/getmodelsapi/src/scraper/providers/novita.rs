//! Novita scraper, mirroring `getmodelsapi/src/scraper/providers/novita.ts`.

use super::ScraperResult;
use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

fn parse_features(features: &[String]) -> Vec<String> {
    let feat: Vec<String> = features.iter().map(|f| f.to_lowercase()).collect();
    let mut result: Vec<String> = Vec::new();
    if feat.iter().any(|f| f.contains("reasoning")) {
        result.push("reasoning".into());
    }
    if feat.iter().any(|f| f.contains("function")) {
        result.push("tools".into());
    }
    if feat.iter().any(|f| f.contains("structured")) {
        result.push("tools".into());
    }
    result.push("chat".into());
    result
}

/// `m.input_token_price_per_m ?? 1` in TS: 0 stays 0, missing => 1.
fn per_m_price(v: Option<&serde_json::Value>) -> f64 {
    v.and_then(|inner| inner.as_f64()).unwrap_or(1.0)
}

/// Scrape the Novita public models endpoint.
pub async fn scrape_novita(_config: ProviderConfig) -> ScraperResult {
    let http_config = HttpConfig {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    match get_json("https://api.novita.ai/v3/openai/models", &http_config).await {
        Ok(value) => {
            let Some(arr) = value.get("data").and_then(|d| d.as_array()) else {
                return ScraperResult::err("Unexpected Novita response");
            };

            let mut models = Vec::with_capacity(arr.len());
            for m in arr {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = m
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&id)
                    .to_string();
                let input_price = m.get("input_token_price_per_m");
                let output_price = m.get("output_token_price_per_m");
                let free_tier = per_m_price(input_price) == 0.0 && per_m_price(output_price) == 0.0;
                let features: Vec<String> = m
                    .get("features")
                    .and_then(|f| f.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                models.push(Model {
                    id: id.clone(),
                    name,
                    provider: "novita".into(),
                    gateway: None,
                    context_window: m
                        .get("context_size")
                        .and_then(|v| v.as_i64())
                        .filter(|&n| n != 0)
                        .unwrap_or(4096),
                    supported_features: parse_features(&features),
                    pricing: Some(Pricing {
                        prompt: input_price.and_then(|v| v.as_f64()).unwrap_or(0.0) / 1e6,
                        completion: output_price.and_then(|v| v.as_f64()).unwrap_or(0.0) / 1e6,
                    }),
                    url: Some(format!("https://novita.ai/models/{id}")),
                    description: m
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.chars().take(300).collect::<String>())
                        .filter(|s| !s.is_empty()),
                    free_tier: free_tier.then_some(true),
                });
            }
            ScraperResult::ok(models)
        }
        Err(e) => ScraperResult::err(e.to_string()),
    }
}