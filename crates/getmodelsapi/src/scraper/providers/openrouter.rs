//! OpenRouter scraper, mirroring `getmodelsapi/src/scraper/providers/openrouter.ts`.

use super::ScraperResult;
use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

fn parse_features(m: &serde_json::Value, id: &str) -> Vec<String> {
    let architecture = m.get("architecture");
    let modality = architecture
        .and_then(|a| a.get("modality"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let inputs: Vec<String> = architecture
        .and_then(|a| a.get("input_modalities"))
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    let has_text = |target: &str| inputs.iter().any(|s| s == target) || modality.contains(target);

    let mut features: Vec<String> = Vec::new();
    if has_text("text") {
        features.push("chat".into());
    }
    if has_text("image") {
        features.push("vision".into());
    }
    if inputs.iter().any(|s| s == "audio") {
        features.push("audio".into());
    }
    if inputs.iter().any(|s| s == "video") {
        features.push("video".into());
    }
    if id.contains("embed") || id.contains("embedding") {
        features.push("embeddings".into());
    }
    if features.is_empty() {
        features.push("chat".into());
    }
    features
}

fn parse_pricing(m: &serde_json::Value) -> Option<Pricing> {
    let pricing = m.get("pricing").filter(|p| !p.is_null())?;
    let p = match pricing.get("prompt") {
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    };
    let c = match pricing.get("completion") {
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    };
    if p == 0.0 && c == 0.0 {
        return None;
    }
    Some(Pricing {
        prompt: p,
        completion: c,
    })
}

/// Scrape the OpenRouter public models endpoint.
pub async fn scrape_openrouter(_config: ProviderConfig) -> ScraperResult {
    let http_config = HttpConfig {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    match get_json("https://openrouter.ai/api/v1/models", &http_config).await {
        Ok(value) => {
            let Some(arr) = value.get("data").and_then(|d| d.as_array()) else {
                return ScraperResult::err("Unexpected OpenRouter API response");
            };

            let mut models = Vec::with_capacity(arr.len());
            for m in arr {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                models.push(Model {
                    id: id.clone(),
                    name,
                    provider: "openrouter".into(),
                    gateway: None,
                    context_window: m
                        .get("context_length")
                        .and_then(|v| v.as_i64())
                        .filter(|&n| n != 0)
                        .unwrap_or(4096),
                    supported_features: parse_features(m, &id),
                    pricing: parse_pricing(m),
                    url: Some(format!("https://openrouter.ai/models/{id}")),
                    description: m
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.chars().take(300).collect::<String>())
                        .filter(|s| !s.is_empty()),
                    free_tier: Some(id.contains("free") || id.contains(":free")),
                });
            }
            ScraperResult::ok(models)
        }
        Err(e) => ScraperResult::err(e.to_string()),
    }
}