//! Google scraper, mirroring `getmodelsapi/src/scraper/providers/google.ts`.

use super::ScraperResult;
use crate::types::{Model, ProviderConfig};
use crate::utils::http::{get_json, get_text, HttpConfig};
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const DEFAULT_CONTEXT: i64 = 32768;

#[derive(Debug, Clone)]
struct CardData {
    id: String,
    raw_id: String,
    name: String,
    description: Option<String>,
}

fn strip_first(s: &str, pat: &str) -> String {
    if let Some(rest) = s.strip_prefix(pat) {
        rest.to_string()
    } else {
        s.to_string()
    }
}

fn regex_trim(s: &str, pat: &str) -> String {
    let re = Regex::new(pat).unwrap();
    if let Some(m) = re.find(s) {
        let mut out = s.to_string();
        out.replace_range(m.start()..m.end(), "");
        out
    } else {
        s.to_string()
    }
}

fn regex_replace(s: &str, pat: &str, to: &str) -> String {
    let re = Regex::new(pat).unwrap();
    if let Some(m) = re.find(s) {
        let mut out = s.to_string();
        out.replace_range(m.start()..m.end(), to);
        out
    } else {
        s.to_string()
    }
}

// Layout 1: `.gemini-model-grid-compact .gemini-model-row`
// Layout 2: `.gemini-centered-model-grid .gemini-card-centered`
fn extract_cards(doc: &Html) -> Vec<CardData> {
    let mut cards: Vec<CardData> = Vec::new();

    for card in doc.select(&Selector::parse(".gemini-model-grid-compact .gemini-model-row").unwrap()) {
        let heading = card
            .select(&Selector::parse("h3[id*=\"gemini\"]").unwrap())
            .next();
        let raw_id = heading.and_then(|h| h.attr("id")).map(|s| s.to_string());
        if let Some(raw_id) = raw_id {
            let name = heading.map(|h| h.text().collect::<String>()).unwrap_or_default();
            let desc = card
                .select(&Selector::parse(".gemini-model-desc").unwrap())
                .next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());
            cards.push(CardData {
                id: normalize_id(&raw_id),
                raw_id,
                name: name.trim().to_string(),
                description: desc,
            });
        }
    }

    for card in doc.select(&Selector::parse(".gemini-centered-model-grid .gemini-card-centered").unwrap()) {
        let heading = card
            .select(&Selector::parse("h3[id*=\"gemini\"]").unwrap())
            .next();
        let raw_id = heading.and_then(|h| h.attr("id")).map(|s| s.to_string());
        if let Some(raw_id) = raw_id {
            let name = heading.map(|h| h.text().collect::<String>()).unwrap_or_default();
            let desc = card
                .select(&Selector::parse(".description-centered").unwrap())
                .next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());
            cards.push(CardData {
                id: normalize_id(&raw_id),
                raw_id,
                name: name.trim().to_string(),
                description: desc,
            });
        }
    }

    cards
}

fn normalize_id(raw_id: &str) -> String {
    let s = regex_trim(raw_id, r"-preview(-\d{2}-\d{4})?$");
    regex_trim(&s, r"-deprecated$")
}

fn parse_context_window(text: &str) -> i64 {
    let re = Regex::new(r"(?i)(\d[\d,]*)\s*[kmM]\s*(?:token|context)").unwrap();
    if let Some(caps) = re.captures(text) {
        if let Some(g) = caps.get(1) {
            let n: i64 = g.as_str().replace(',', "").parse().unwrap_or(0);
            let whole = caps.get(0).map(|c| c.as_str()).unwrap_or("").to_lowercase();
            if whole.contains('m') {
                return n * 1_000_000;
            }
            if whole.contains('k') {
                return n * 1000;
            }
            return n;
        }
    }
    DEFAULT_CONTEXT
}

fn deduce_features(name: &str, desc: &str, id: &str) -> Vec<String> {
    let text = format!("{name} {desc} {id}").to_lowercase();

    if id.contains("embedding") || text.contains("embedding") {
        return vec!["embeddings".into()];
    }

    let mut features: Vec<String> = Vec::new();
    if text.contains("vision") || text.contains("image") || text.contains("multimodal") {
        features.push("vision".into());
    }
    if text.contains("audio")
        || text.contains("voice")
        || (text.contains("speech") && !text.contains("speech synthesis"))
    {
        features.push("audio".into());
    }
    if text.contains("video") {
        features.push("video".into());
    }
    if text.contains("code") || text.contains("coding") {
        features.push("code".into());
    }
    if text.contains("text-to-speech")
        || text.contains("speech synthesis")
        || text.contains("speech generation")
    {
        features.push("tts".into());
    }
    if text.contains("reasoning") {
        features.push("reasoning".into());
    }

    features.push("chat".into());
    features
}

fn normalize_for_match(id: &str) -> String {
    let mut s = strip_first(id, "google/");
    s = s.replace('.', "-");
    // Order matches the TS `.replace` chain.
    let suffix_pats = [
        r"-preview.*$",
        r"-deprecated$",
        r"-latest$",
        r"-0\d{3}$",
        r"-2$",
        r"-shut-down$",
        r"-tts$",
        r"-live$",
        r"-lite$",
        r"-pro$",
        r"-flash$",
        r"-max$",
        r"-research$",
        r"-image$",
        r"-robotics$",
        r"-embedding.*$",
    ];
    for pat in suffix_pats {
        if pat == r"-embedding.*$" {
            s = regex_replace(&s, pat, "-embedding");
        } else if let Some(m) = Regex::new(pat).unwrap().find(&s) {
            let mut tmp = s.clone();
            tmp.replace_range(m.start()..m.end(), "");
            s = tmp;
        }
    }
    // collapse double dashes
    let re = Regex::new(r"--+").unwrap();
    s = re.replace_all(&s, "-").into_owned();
    while s.ends_with('-') {
        s.pop();
    }
    s
}

async fn get_context_from_openrouter() -> HashMap<String, i64> {
    let http_config = HttpConfig {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let mut map = HashMap::new();
    if let Ok(value) = get_json("https://openrouter.ai/api/v1/models", &http_config).await {
        if let Some(arr) = value.get("data").and_then(|d| d.as_array()) {
            for m in arr {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let ctx = m.get("context_length").and_then(|v| v.as_i64()).unwrap_or(0);
                if id.starts_with("google/") && ctx > 0 {
                    let key = normalize_for_match(id);
                    let existing = map.get(&key).copied().unwrap_or(0);
                    // Prefer higher context window if multiple models map to
                    // the same key.
                    if existing == 0 || ctx > existing {
                        map.insert(key, ctx);
                    }
                }
            }
        }
    }
    map
}

async fn scrape_google_page() -> Result<Vec<Model>, String> {
    let resp = get_text(
        "https://ai.google.dev/gemini-api/docs/models",
        &HttpConfig {
            headers: vec![
                ("User-Agent".into(), "Mozilla/5.0 GetModels".into()),
                ("Accept-Language".into(), "en-US,en;q=0.9".into()),
            ],
            timeout: Duration::from_secs(15),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let ctx_map = get_context_from_openrouter().await;
    let doc = Html::parse_document(&resp);
    let cards = extract_cards(&doc);
    let mut seen: HashSet<String> = HashSet::new();
    let mut models: Vec<Model> = Vec::new();

    for c in cards {
        if seen.contains(&c.id) {
            continue;
        }
        seen.insert(c.id.clone());

        let scraped_ctx = parse_context_window(c.description.as_deref().unwrap_or(""));
        let or_ctx = ctx_map
            .get(&normalize_for_match(&c.id))
            .copied()
            .or_else(|| ctx_map.get(&normalize_for_match(&c.raw_id)).copied());
        let context_window = or_ctx.unwrap_or(scraped_ctx);

        models.push(Model {
            id: c.id.clone(),
            name: c.name,
            provider: "google".into(),
            gateway: None,
            context_window,
            supported_features: deduce_features(&c.id, c.description.as_deref().unwrap_or(""), &c.id),
            pricing: None,
            url: Some(format!(
                "https://ai.google.dev/gemini-api/docs/models#{}",
                c.raw_id
            )),
            description: c
                .description
                .map(|s| s.chars().take(300).collect::<String>())
                .filter(|s| !s.is_empty()),
            free_tier: Some(true),
        });
    }

    Ok(models)
}

fn features_from_methods(m: &serde_json::Value, name: &str) -> Vec<String> {
    let methods: Vec<String> = m
        .get("supportedGenerationMethods")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let mut features: Vec<String> = Vec::new();
    if methods
        .iter()
        .any(|mt| mt.contains("generatecontent") || mt.contains("generatemessage"))
    {
        features.push("chat".into());
    }
    if methods.iter().any(|mt| mt.contains("vision") || mt.contains("image")) {
        features.push("vision".into());
    }
    if methods.iter().any(|mt| mt.contains("audio") || mt.contains("speech")) {
        features.push("audio".into());
    }
    if methods.iter().any(|mt| mt.contains("video")) {
        features.push("video".into());
    }
    if name.contains("embedding") {
        features.push("embeddings".into());
    }
    if features.is_empty() {
        features.push("chat".into());
    }
    features
}

/// Mirror of `scrapeGoogle`: if an API key is configured, try the API first,
/// then fall through to scraping the docs page.
pub async fn scrape_google(config: ProviderConfig) -> ScraperResult {
    if let Some(api_key) = config.api_key.as_deref() {
        let url = format!("{}/models", config.base_url);
        let http_config = HttpConfig {
            params: vec![("key".into(), api_key.to_string())],
            timeout: Duration::from_secs(10),
            ..Default::default()
        };
        if let Ok(value) = get_json(&url, &http_config).await {
            let mut models = Vec::new();
            if let Some(arr) = value.get("models").and_then(|d| d.as_array()) {
                for m in arr {
                    let full_name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let id = full_name.replacen("models/", "", 1);
                    let name = m
                        .get("displayName")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(full_name)
                        .trim();
                    models.push(Model {
                        id: id.clone(),
                        name: name.to_string(),
                        provider: "google".into(),
                        gateway: None,
                        context_window: m
                            .get("inputTokenLimit")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(DEFAULT_CONTEXT),
                        supported_features: features_from_methods(m, full_name),
                        pricing: None,
                        url: Some(format!(
                            "https://ai.google.dev/gemini-api/docs/models#{id}"
                        )),
                        description: m
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        free_tier: Some(true),
                    });
                }
            }
            return ScraperResult::ok(models);
        }
        // fall through to scraper
    }

    match scrape_google_page().await {
        Ok(models) => ScraperResult::ok(models),
        Err(e) => ScraperResult::err(e.to_string()),
    }
}