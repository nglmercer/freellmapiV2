//! Port of `server/src/services/rankings/match.ts`.
//!
//! Exact-match model ID resolution across heterogeneous provider
//! namespaces. No fuzzy matching. If the model isn't in the source, the row
//! stays unranked.

use std::collections::HashMap;

use std::sync::LazyLock;

use regex::Regex;

#[derive(Clone, Debug)]
pub struct ModelIdentity {
    /// original id from the local DB, e.g. "google/gemini-2.5-pro"
    pub raw: String,
    /// lowercased, ready for case-insensitive equality checks
    pub lower: String,
    /// id with the publisher prefix stripped, e.g. "gemini-2.5-pro"
    pub bare: String,
    /// bare id, lowercased
    pub bare_lower: String,
    /// bare id with trailing ":free" / "-free" / version tags stripped
    pub canonical: String,
    /// canonical, lowercased
    pub canonical_lower: String,
}

pub struct RankLookup {
    index: HashMap<String, crate::services::rankings::types::SourceBenchmark>,
}

impl RankLookup {
    /// Look up a ranking for the given local model — strict equality only.
    pub fn find(&self, id: &ModelIdentity) -> Option<crate::services::rankings::types::SourceBenchmark> {
        self.index
            .get(&id.lower)
            .or_else(|| self.index.get(&id.bare_lower))
            .or_else(|| self.index.get(&id.canonical_lower))
            .cloned()
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

fn strip_publisher(id: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[^/\s]+/"));
    RE.replace(id, "").to_string()
}

fn canonicalize(id: &str) -> String {
    static FREE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)[:-]free$"));
    static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"-\d{4}-\d{2}-\d{2}$"));
    static PREVIEW: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)-(preview|alpha|beta|exp)(\.\d+)?$"));
    static PROVIDER: LazyLock<Regex> = LazyLock::new(|| re(r"(?i):nitro$|:exacto$|:floor$"));
    static SHA: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)@[a-f0-9]{6,40}$"));

    let s = FREE.replace(id, "");
    let s = DATE.replace(&s, "");
    let s = PREVIEW.replace(&s, "");
    let s = SHA.replace(&s, "");
    let s = PROVIDER.replace(&s, "");
    s.to_string()
}

pub fn identity(_platform: &str, model_id: &str) -> ModelIdentity {
    let raw = model_id.to_string();
    let lower = model_id.to_lowercase();
    let bare = strip_publisher(model_id);
    let bare_lower = bare.to_lowercase();
    let canonical = canonicalize(&bare);
    let canonical_lower = canonical.to_lowercase();
    ModelIdentity {
        raw,
        lower,
        bare,
        bare_lower,
        canonical,
        canonical_lower,
    }
}

pub fn build_rank_index(
    rows: impl IntoIterator<Item = (String, crate::services::rankings::types::SourceBenchmark)>,
) -> RankLookup {
    let mut index = HashMap::new();
    for (key, value) in rows {
        index.insert(key.to_lowercase(), value);
    }
    RankLookup { index }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_strips_prefix_and_free_suffix() {
        let id = identity("openrouter", "meta-llama/llama-3.3-70b-instruct:free");
        assert_eq!(id.bare, "llama-3.3-70b-instruct:free");
        assert_eq!(id.canonical, "llama-3.3-70b-instruct");
    }

    #[test]
    fn lookup_prefers_full_then_bare_then_canonical() {
        use crate::services::rankings::types::SourceBenchmark;
        let b = SourceBenchmark { intelligence_score: 50.0, speed_tokens_per_sec: 100.0 };
        let lookup = build_rank_index(vec![("llama-3.3-70b-instruct".to_string(), b.clone())]);
        let id = identity("openrouter", "meta-llama/LLaMA-3.3-70B-Instruct:free");
        assert_eq!(lookup.find(&id).unwrap().intelligence_score, 50.0);
    }

    #[test]
    fn canonicalization_tags() {
        assert_eq!(canonicalize("gpt-4.1-mini-2024-04-14"), "gpt-4.1-mini");
        assert_eq!(canonicalize("gemini-2.5-pro-preview"), "gemini-2.5-pro");
        assert_eq!(canonicalize("gemini-2.5-pro-preview.2"), "gemini-2.5-pro");
        assert_eq!(canonicalize("model@abc123def"), "model");
        assert_eq!(canonicalize("model:nitro"), "model");
        assert_eq!(canonicalize("llama-3.3:free"), "llama-3.3");
    }
}
