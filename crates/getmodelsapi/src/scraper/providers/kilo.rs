//! Kilo scraper.

use super::ScraperResult;
use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

fn parse_features(id: &str, name: &str, input_modalities: &[String]) -> Vec<String> {
    let inputs: Vec<String> = input_modalities.iter().map(|s| s.to_lowercase()).collect();
    let mut features: Vec<String> = Vec::new();
    if inputs.iter().any(|s| s == "text") {
        features.push("chat".into());
    }
    if inputs.iter().any(|s| s == "image") {
        features.push("vision".into());
    }
    if inputs.iter().any(|s| s == "file") {
        features.push("file".into());
    }
    if inputs.iter().any(|s| s == "audio") {
        features.push("audio".into());
    }
    let lower_name = name.to_lowercase();
    if id.contains("embed") || lower_name.contains("embed") {
        features.push("embeddings".into());
    }
    if features.is_empty() {
        features.push("chat".into());
    }
    features
}

/// Parse a provider numeric field, defaulting missing or invalid values to 0.
fn parse_price(v: Option<&serde_json::Value>) -> f64 {
    let s = match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|f| f.to_string()).unwrap_or_default(),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    if s.is_empty() {
        0.0
    } else {
        s.trim().parse().unwrap_or(0.0)
    }
}

/// Scrape the Kilo models API.
pub async fn scrape_kilo(_config: ProviderConfig) -> ScraperResult {
    let http_config = HttpConfig {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    match get_json("https://kilo.ai/api/models", &http_config).await {
        Ok(value) => {
            let Some(arr) = value.as_array() else {
                return ScraperResult::err("Unexpected Kilo API response");
            };

            let mut models = Vec::with_capacity(arr.len());
            for m in arr {
                let id = m
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let openrouter_id = m
                    .get("openrouterId")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let model_id = openrouter_id.clone().unwrap_or_else(|| id.clone());
                let name = m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let slash = m
                    .get("slug")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&id);
                let creator = m
                    .get("modelCreator")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "kilo".into());
                let prompt_price = parse_price(m.get("priceInput"));
                let completion_price = parse_price(m.get("priceOutput"));
                // Invalid numeric text is treated as an absent value.
                let p_falsy = prompt_price == 0.0 || prompt_price.is_nan();
                let c_falsy = completion_price == 0.0 || completion_price.is_nan();
                let free_tier =
                    (prompt_price == 0.0 && completion_price == 0.0) || (p_falsy && c_falsy);
                let input_modalities: Vec<String> = m
                    .get("inputModalities")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                models.push(Model {
                    id: model_id,
                    name,
                    provider: "kilo".into(),
                    gateway: (creator != "kilo").then_some(creator),
                    context_window: m
                        .get("contextLength")
                        .and_then(|v| v.as_i64())
                        .filter(|&n| n != 0)
                        .unwrap_or(4096),
                    supported_features: parse_features(
                        &id,
                        m.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        &input_modalities,
                    ),
                    pricing: if prompt_price > 0.0 || completion_price > 0.0 {
                        Some(Pricing {
                            prompt: prompt_price,
                            completion: completion_price,
                        })
                    } else {
                        None
                    },
                    url: Some(format!("https://kilo.ai/models/{slash}")),
                    description: m
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.chars().take(300).collect::<String>())
                        .filter(|s| !s.is_empty()),
                    free_tier: Some(free_tier),
                });
            }
            ScraperResult::ok(models)
        }
        Err(e) => ScraperResult::err(e.to_string()),
    }
}
