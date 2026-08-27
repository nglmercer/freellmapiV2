//! SambaNova scraper, mirroring `getmodelsapi/src/scraper/providers/sambanova.ts`.

use super::ScraperResult;
use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

fn parse_float(v: Option<&serde_json::Value>) -> f64 {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Scrape the SambaNova public models endpoint.
pub async fn scrape_sambanova(_config: ProviderConfig) -> ScraperResult {
    let http_config = HttpConfig {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    match get_json("https://api.sambanova.ai/v1/models", &http_config).await {
        Ok(value) => {
            let Some(arr) = value.get("data").and_then(|d| d.as_array()) else {
                return ScraperResult::err("Unexpected SambaNova response");
            };

            let mut models = Vec::with_capacity(arr.len());
            for m in arr {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let pricing_json = m.get("pricing").filter(|p| !p.is_null());
                let pricing = pricing_json.map(|p| Pricing {
                    prompt: parse_float(p.get("prompt")),
                    completion: parse_float(p.get("completion")),
                });

                models.push(Model {
                    id: id.clone(),
                    name: id.clone(),
                    provider: "sambanova".into(),
                    gateway: None,
                    context_window: m
                        .get("context_length")
                        .and_then(|v| v.as_i64())
                        .filter(|&n| n != 0)
                        .unwrap_or(4096),
                    supported_features: vec!["chat".into()],
                    pricing,
                    url: Some(format!("https://cloud.sambanova.ai/models/{id}")),
                    description: None,
                    free_tier: None,
                });
            }
            ScraperResult::ok(models)
        }
        Err(e) => ScraperResult::err(e.to_string()),
    }
}