//! Port of `server/src/services/model-sync/mappings.ts`.

use std::collections::HashMap;

use crate::db::seed::{UNRANKED_INTELLIGENCE, UNRANKED_SPEED};

pub fn provider_to_platform_map() -> HashMap<&'static str, Option<&'static str>> {
    let mut m = HashMap::new();
    m.insert("google", Some("google"));
    m.insert("mistral", Some("mistral"));
    m.insert("openrouter", Some("openrouter"));
    m.insert("groq", Some("groq"));
    m.insert("cohere", Some("cohere"));
    m.insert("sambanova", Some("sambanova"));
    m.insert("kilo", Some("kilo"));
    m.insert("together", None);
    m.insert("aimlapi", None);
    m.insert("novita", None);
    m.insert("huggingface", None);
    m
}

/// Sentinels used when no real benchmark has been applied. The enrichment
/// service (services/rankings/enrich.rs) overwrites these with real scores
/// fetched from external sources. There is intentionally no name-based
/// fallback here — see enrich.rs for the rationale.
#[derive(Clone, Copy, Debug)]
pub struct CurationDefaults {
    pub intelligence_rank: i64,
    pub speed_rank: i64,
    pub size_label: &'static str,
    pub rpm_limit: Option<i64>,
    pub rpd_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub tpd_limit: Option<i64>,
    pub monthly_token_budget: &'static str,
    pub enabled: i64,
}

pub const CURATION_DEFAULTS: CurationDefaults = CurationDefaults {
    intelligence_rank: UNRANKED_INTELLIGENCE,
    speed_rank: UNRANKED_SPEED,
    size_label: "",
    rpm_limit: None,
    rpd_limit: None,
    tpm_limit: None,
    tpd_limit: None,
    monthly_token_budget: "",
    enabled: 1,
};

pub fn get_platform_by_provider(raw_provider: &str) -> Option<&'static str> {
    provider_to_platform_map()
        .get(raw_provider)
        .copied()
        .flatten()
}
