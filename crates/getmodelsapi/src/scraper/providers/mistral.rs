//! Mistral scraper.

use super::ScraperResult;
use crate::types::{Model, ProviderConfig};
use crate::utils::http::{get_json, get_text, HttpConfig};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const DEFAULT_CONTEXT: i64 = 32768;

fn nav_headings_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)why mistral|explore|documentation|build|legal|community|getting started|overview|quickstart|api reference|sdks|index|featured models|frontier models|generalist|specialist|other models|legacy|deprecated|search",
        )
        .unwrap()
    })
}

fn model_heading_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)mistral|pixtral|ministral|codestral|voxtral|magistral|devstral|leanstral|embed|ocr|moderation",
        )
        .unwrap()
    })
}

/// Parse Mistral context-window text using the supported page patterns.
fn parse_context_window(text: &str) -> i64 {
    struct Pattern {
        re: Regex,
        k_mult: bool,
        million_mult: bool,
    }
    let patterns = [
        Pattern {
            re: Regex::new(r"(?i)(\d+)\s*m(?:illion)?\s*(?:token|context)").unwrap(),
            k_mult: false,
            million_mult: true, // pattern string contains "million"
        },
        Pattern {
            re: Regex::new(r"(?i)(\d+)\s*tokens?\s*(?:context|window)").unwrap(),
            k_mult: true, // pattern string contains "tokens?" (has a "k")
            million_mult: false,
        },
        Pattern {
            re: Regex::new(r"(?i)context\s*(?:window|length).*?(\d[\d,]*)\s*[km]").unwrap(),
            k_mult: true, // pattern string contains "[km]"
            million_mult: false,
        },
        Pattern {
            re: Regex::new(r"(?i)(\d+)\s*k\s*(?:token|context)").unwrap(),
            k_mult: true,
            million_mult: false,
        },
        Pattern {
            re: Regex::new(r"(?i)(\d[\d,]*)\s*token").unwrap(),
            k_mult: false,
            million_mult: false,
        },
    ];
    for p in &patterns {
        if let Some(caps) = p.re.captures(text) {
            if let Some(g) = caps.get(1) {
                let n: i64 = g.as_str().replace(',', "").parse().unwrap_or(0);
                let mut n = n;
                if p.k_mult {
                    n *= 1000;
                }
                let whole = caps.get(0).map(|c| c.as_str()).unwrap_or("");
                if p.million_mult || whole.to_lowercase().contains('m') {
                    n *= 1_000_000;
                }
                return n;
            }
        }
    }
    DEFAULT_CONTEXT
}

fn deduce_features(id: &str, description: &str, name: &str) -> Vec<String> {
    let text = format!("{name} {description} {id}").to_lowercase();

    if text.contains("embed") || text.contains("embedding") {
        return vec!["embeddings".into()];
    }
    if text.contains("moderation") {
        return vec!["moderation".into()];
    }
    if text.contains("ocr") {
        return vec!["ocr".into()];
    }
    if text.contains("tts")
        || text.contains("transcribe")
        || text.contains("speech")
        || text.contains("audio")
    {
        return vec!["audio".into(), "tts".into()];
    }

    let mut features: Vec<String> = Vec::new();
    if text.contains("vision") || text.contains("image") || id.contains("pixtral") {
        features.push("vision".into());
    }
    if text.contains("code")
        || id.contains("codestral")
        || id.contains("devstral")
        || id.contains("leanstral")
    {
        features.push("code".into());
    }

    features.push("chat".into());
    features
}

fn regex_replace_first(s: &str, pat: &str) -> String {
    let re = Regex::new(pat).unwrap();
    if let Some(m) = re.find(s) {
        let mut out = s.to_string();
        out.replace_range(m.start()..m.end(), "");
        out
    } else {
        s.to_string()
    }
}

fn strip_prefix_str<'a>(s: &'a str, prefix: &str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}

fn normalize_for_match(text: &str) -> String {
    let mut s = text.to_string();
    s = strip_prefix_str(&s, "mistral/").to_string();
    s = strip_prefix_str(&s, "mistralai/").to_string();
    s = regex_replace_first(&s, r"(?i)^mistral:\s*");
    s = regex_replace_first(&s, r"^\d{4}\s+");
    s = regex_replace_first(&s, r"\s+\d{4}$");
    s = regex_replace_first(&s, r"(?i)\s+\d+b$");
    s = regex_replace_first(&s, r"-latest$");
    s = regex_replace_first(&s, r"-(\d{4}-\d{2}-\d{2})$");
    let re = Regex::new(r"--+").unwrap();
    s = re.replace_all(&s, "-").into_owned();
    while s.ends_with('-') {
        s.pop();
    }
    s.to_lowercase()
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
                let ctx = m
                    .get("context_length")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if (id.starts_with("mistral/") || id.starts_with("mistralai/")) && ctx > 0 {
                    let name_key =
                        normalize_for_match(m.get("name").and_then(|v| v.as_str()).unwrap_or(""));
                    let id_key = normalize_for_match(id);
                    let existing = map.get(&name_key).copied().unwrap_or(0);
                    if existing == 0 || ctx > existing {
                        map.insert(name_key, ctx);
                    }
                    let id_existing = map.get(&id_key).copied().unwrap_or(0);
                    if id_existing == 0 || ctx > id_existing {
                        map.insert(id_key, ctx);
                    }
                }
            }
        }
    }
    map
}

/// All sibling elements after `el`, stopping at the next `h2`/`h3`/`h4`
/// (cheerio `nextUntil("h2, h3, h4")`).
fn next_until<'a>(el: &ElementRef<'a>) -> Vec<ElementRef<'a>> {
    let mut out: Vec<ElementRef<'a>> = Vec::new();
    let mut cur = el.next_sibling();
    while let Some(node) = cur {
        if let Some(e) = ElementRef::wrap(node) {
            let name: &str = e.value().name();
            if name == "h2" || name == "h3" || name == "h4" {
                break;
            }
            out.push(e);
        }
        cur = node.next_sibling();
    }
    out
}

async fn scrape_mistral_page() -> Result<Vec<Model>, String> {
    let resp = get_text(
        "https://docs.mistral.ai/getting-started/models/",
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

    // Collect model API IDs from the deprecation table (col index 2).
    let mut table_models: Vec<(String, String)> = Vec::new(); // display name -> apiId
    let mut table_seen: HashSet<String> = HashSet::new();
    let table_selector = Selector::parse("table").unwrap();
    let row_selector = Selector::parse("tr").unwrap();
    let cell_selector = Selector::parse("td").unwrap();
    let clean_name_re = Regex::new(r"\s*↗$").unwrap();
    if let Some(last_table) = doc.select(&table_selector).next_back() {
        for row in last_table.select(&row_selector) {
            let cells: Vec<ElementRef> = row.select(&cell_selector).collect();
            if cells.len() < 3 {
                continue;
            }
            let display_name = cells[0].text().collect::<String>().trim().to_string();
            let api_id = cells[2].text().collect::<String>().trim().to_string();
            if !api_id.is_empty()
                && !display_name.is_empty()
                && api_id.chars().count() > 3
                && !api_id.contains(' ')
                && display_name.chars().count() > 3
                && display_name != "Model"
            {
                // Remove the " ↗" suffix if present.
                let clean_name = clean_name_re.replace(&display_name, "").trim().to_string();
                if !table_seen.contains(&clean_name) {
                    table_seen.insert(clean_name.clone());
                    table_models.push((clean_name, api_id));
                }
            }
        }
    }

    // Extract current models from headings.
    let heading_selector = Selector::parse("h2, h3, h4").unwrap();
    let ws_re = Regex::new(r"\s+").unwrap();
    let bad_re = Regex::new(r"[^a-z0-9-]").unwrap();
    let mut models: Vec<Model> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for el in doc.select(&heading_selector) {
        let text = el.text().collect::<String>().trim().to_string();
        if text.is_empty() || text.chars().count() < 3 {
            continue;
        }
        if nav_headings_re().is_match(&text) {
            continue;
        }
        if !model_heading_re().is_match(&text) {
            continue;
        }

        let name = text;
        let section = next_until(&el);
        let description = section
            .iter()
            .find(|e| {
                let name: &str = e.value().name();
                name == "p"
            })
            .map(|p| p.text().collect::<String>().trim().to_string());
        let section_text = description.clone().unwrap_or_default();

        // Generate a clean ID from the API table or the heading.
        let mut id = name.to_lowercase();
        id = ws_re.replace_all(&id, "-").into_owned();
        id = bad_re.replace_all(&id, "").into_owned();
        if let Some((_, table_id)) = table_models.iter().find(|(d, _)| d == &name) {
            id = table_id.clone();
        }

        if seen.contains(&id) {
            continue;
        }
        seen.insert(id.clone());

        let scraped_ctx = parse_context_window(&section_text);
        let or_ctx = ctx_map
            .get(&normalize_for_match(&name))
            .copied()
            .or_else(|| ctx_map.get(&id).copied())
            .or_else(|| ctx_map.get(&normalize_for_match(&id)).copied());
        let context_window = or_ctx.unwrap_or(scraped_ctx);
        let features = deduce_features(&id, &section_text, &name);

        models.push(Model {
            id: id.clone(),
            name,
            provider: "mistral".into(),
            gateway: None,
            context_window,
            supported_features: features,
            pricing: None,
            url: Some(format!(
                "https://docs.mistral.ai/getting-started/models/#{id}"
            )),
            description,
            free_tier: Some(true),
        });
    }

    // Fallback: if heading extraction found little, use table data directly.
    if models.is_empty() {
        for (display_name, api_id) in &table_models {
            if seen.contains(api_id) {
                continue;
            }
            seen.insert(api_id.clone());

            // Skip labs/experimental.
            if api_id.starts_with("labs-") {
                continue;
            }

            models.push(Model {
                id: api_id.clone(),
                name: display_name.clone(),
                provider: "mistral".into(),
                gateway: None,
                context_window: DEFAULT_CONTEXT,
                supported_features: deduce_features(api_id, "", display_name),
                pricing: None,
                url: Some(format!(
                    "https://docs.mistral.ai/getting-started/models/#{api_id}"
                )),
                description: None,
                free_tier: Some(true),
            });
        }
    }

    Ok(models)
}

/// Try the API when an API key exists, otherwise scrape the documentation
/// page.
pub async fn scrape_mistral(config: ProviderConfig) -> ScraperResult {
    if let Some(api_key) = config.api_key.as_deref() {
        let url = format!("{}/models", config.base_url);
        let http_config = HttpConfig {
            headers: vec![("Authorization".into(), format!("Bearer {api_key}"))],
            timeout: Duration::from_secs(10),
            ..Default::default()
        };
        if let Ok(value) = get_json(&url, &http_config).await {
            let mut models = Vec::new();
            if let Some(arr) = value.get("data").and_then(|d| d.as_array()) {
                for m in arr {
                    let name = m
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    models.push(Model {
                        id: name.clone(),
                        name: name.clone(),
                        provider: "mistral".into(),
                        gateway: None,
                        context_window: m
                            .get("max_context_length")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(DEFAULT_CONTEXT),
                        supported_features: vec!["chat".into()],
                        pricing: None,
                        url: Some(format!(
                            "https://docs.mistral.ai/getting-started/models/#{name}"
                        )),
                        description: m
                            .get("owned_by")
                            .and_then(|v| v.as_str())
                            .map(|o| format!("Owned by {o}")),
                        free_tier: Some(true),
                    });
                }
            }
            return ScraperResult::ok(models);
        }
        tracing::error!("Mistral API failed, falling back to scraper");
    }

    match scrape_mistral_page().await {
        Ok(models) => ScraperResult::ok(models),
        Err(e) => ScraperResult::err(e.to_string()),
    }
}
