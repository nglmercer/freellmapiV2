//! Deterministic model identity helpers.
//!
//! Matching is deliberately exact at each resolution level. Normalization is
//! limited to documented transport decorations (`:free`, provider routing
//! suffixes, dated/preview tags); it is never a similarity score. Ambiguous
//! normalized keys are rejected instead of merged.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::services::rankings::types::SourceBenchmark;

#[derive(Clone, Debug)]
pub struct ModelIdentity {
    pub raw: String,
    pub lower: String,
    pub bare: String,
    pub bare_lower: String,
    pub canonical: String,
    pub canonical_lower: String,
}

pub struct RankLookup {
    index: HashMap<String, SourceBenchmark>,
    ambiguous: HashSet<String>,
}

impl RankLookup {
    /// Look up a ranking using exact full, bare, then safely normalized IDs.
    pub fn find(&self, id: &ModelIdentity) -> Option<SourceBenchmark> {
        [&id.lower, &id.bare_lower, &id.canonical_lower]
            .into_iter()
            .find_map(|key| {
                if self.ambiguous.contains(key) {
                    None
                } else {
                    self.index.get(key).cloned()
                }
            })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

fn strip_publisher(id: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[^/\s]+/"));
    RE.replace(id, "").to_string()
}

/// Strip only transport/version decorations that do not identify a different
/// model. Model-family variants such as `coder`, `thinking`, and parameter
/// counts are intentionally preserved.
pub fn canonicalize(id: &str) -> String {
    static FREE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)[:-]free$"));
    static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"-\d{4}-\d{2}-\d{2}$"));
    static PREVIEW: LazyLock<Regex> =
        LazyLock::new(|| re(r"(?i)-(preview|alpha|beta|exp)(\.\d+)?$"));
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

/// Return the stable registry key used for a local provider model. This is a
/// conservative bare/canonical ID, not a fuzzy similarity result.
pub fn canonical_id_for_local(platform: &str, model_id: &str) -> String {
    let platform_key = platform.to_ascii_lowercase();
    let local_key = model_id.to_ascii_lowercase();
    for &(known_platform, known_id, canonical) in CURATED_CANONICAL_ALIASES {
        if platform_key == known_platform && local_key == known_id {
            return canonical.to_string();
        }
    }
    // Keep a publisher prefix when one is present. A bare model name such as
    // `gpt-4o` is not enough evidence to merge unrelated publishers; reviewed
    // provider aliases above are the mechanism for intentional cross-provider
    // identity joins.
    canonicalize(&local_key)
}

/// A small, reviewed map for provider IDs that are known to refer to the
/// same underlying model despite provider-specific naming. Unknown variants
/// remain separate until an explicit alias is added.
const CURATED_CANONICAL_ALIASES: &[(&str, &str, &str)] = &[
    (
        "groq",
        "llama-3.3-70b-versatile",
        "meta-llama/llama-3.3-70b-instruct",
    ),
    (
        "sambanova",
        "meta-llama-3.3-70b-instruct",
        "meta-llama/llama-3.3-70b-instruct",
    ),
    (
        "openrouter",
        "meta-llama/llama-3.3-70b-instruct:free",
        "meta-llama/llama-3.3-70b-instruct",
    ),
];

pub fn publisher_from_model_id(model_id: &str) -> Option<String> {
    model_id
        .split_once('/')
        .map(|(publisher, _)| publisher.to_string())
}

fn insert_key(
    index: &mut HashMap<String, SourceBenchmark>,
    ambiguous: &mut HashSet<String>,
    key: String,
    value: &SourceBenchmark,
) {
    if ambiguous.contains(&key) {
        return;
    }
    match index.get(&key) {
        None => {
            index.insert(key, value.clone());
        }
        Some(existing) if existing == value => {}
        Some(_) => {
            index.remove(&key);
            ambiguous.insert(key);
        }
    }
}

/// Build a strict lookup index. Every source key is indexed by its exact form
/// and its safe bare/canonical forms; collisions with different scores become
/// unresolved rather than silently choosing one model.
pub fn build_rank_index(rows: impl IntoIterator<Item = (String, SourceBenchmark)>) -> RankLookup {
    let mut index = HashMap::new();
    let mut ambiguous = HashSet::new();
    for (key, value) in rows {
        let id = identity("source", &key);
        for normalized in [id.lower, id.bare_lower, id.canonical_lower] {
            insert_key(&mut index, &mut ambiguous, normalized, &value);
        }
    }
    RankLookup { index, ambiguous }
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
        let b = SourceBenchmark {
            intelligence_score: Some(50.0),
            speed_tokens_per_sec: Some(100.0),
        };
        let lookup = build_rank_index(vec![("llama-3.3-70b-instruct".to_string(), b.clone())]);
        let id = identity("openrouter", "meta-llama/LLaMA-3.3-70B-Instruct:free");
        assert_eq!(lookup.find(&id).unwrap().intelligence_score, Some(50.0));
    }

    #[test]
    fn canonicalization_preserves_model_variants() {
        assert_eq!(canonicalize("gpt-4.1-mini-2024-04-14"), "gpt-4.1-mini");
        assert_eq!(canonicalize("gemini-2.5-pro-preview"), "gemini-2.5-pro");
        assert_eq!(canonicalize("gemini-2.5-pro-preview.2"), "gemini-2.5-pro");
        assert_eq!(canonicalize("model@abc123def"), "model");
        assert_eq!(canonicalize("model:nitro"), "model");
        assert_eq!(canonicalize("llama-3.3:free"), "llama-3.3");
        assert_ne!(
            canonicalize("qwen-3-coder"),
            canonicalize("qwen-3-thinking")
        );
    }

    #[test]
    fn ambiguous_normalized_keys_are_not_matched() {
        let lookup = build_rank_index(vec![
            (
                "qwen-3-coder".to_string(),
                SourceBenchmark {
                    intelligence_score: Some(50.0),
                    speed_tokens_per_sec: None,
                },
            ),
            (
                "QWEN-3-CODER".to_string(),
                SourceBenchmark {
                    intelligence_score: Some(51.0),
                    speed_tokens_per_sec: None,
                },
            ),
        ]);
        assert!(lookup.find(&identity("provider", "qwen-3-coder")).is_none());
    }

    #[test]
    fn curated_aliases_are_explicit_and_variants_stay_distinct() {
        assert_eq!(
            canonical_id_for_local("groq", "llama-3.3-70b-versatile"),
            "meta-llama/llama-3.3-70b-instruct"
        );
        assert_ne!(
            canonical_id_for_local("provider", "qwen-3-coder"),
            canonical_id_for_local("provider", "qwen-3-thinking")
        );
        assert_ne!(
            canonical_id_for_local("provider", "openai/gpt-4o"),
            canonical_id_for_local("provider", "anthropic/gpt-4o")
        );
    }
}
