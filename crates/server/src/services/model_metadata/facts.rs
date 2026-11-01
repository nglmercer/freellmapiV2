//! Effective registry values for a set of deployments, read in one query.
//!
//! A deployment's own value wins over its canonical model's value, the same
//! precedence the Models page uses. Quality prefers a reviewed manual score
//! over the Artificial Analysis score.

use std::collections::HashMap;

use rusqlite::{params_from_iter, Connection};
use serde_json::{json, Value};

/// Quality metrics in preference order.
pub const QUALITY_METRICS: [&str; 2] =
    ["benchmark.manual.intelligence", "benchmark.aa.intelligence"];
pub const SPEED_METRIC: &str = "performance.output_tps";
const CODING_METRIC: &str = "benchmark.aa.coding";
const CONTEXT_METRIC: &str = "context.total";
const PRICE_INPUT_METRIC: &str = "price.input";
const PRICE_OUTPUT_METRIC: &str = "price.output";
const TTFT_METRIC: &str = "performance.ttft_ms";
const LATENCY_METRIC: &str = "performance.latency_ms";
const CAPABILITY_PREFIX: &str = "capability.";

/// One effective value with the evidence the UI shows next to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    pub value: f64,
    pub source: String,
    pub confidence: f64,
    pub fetched_at: String,
}

/// The registry facts the fallback chain shows for one deployment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelFacts {
    pub quality: Option<Fact>,
    pub speed: Option<Fact>,
    pub coding: Option<f64>,
    pub context_window: Option<f64>,
    pub price_input: Option<f64>,
    pub price_output: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub latency_ms: Option<f64>,
    /// Capability names the registry marks as supported, sorted.
    pub capabilities: Vec<String>,
}

impl ModelFacts {
    /// The `metrics` object sent to the client. Missing values are `null`.
    pub fn to_json(&self) -> Value {
        json!({
            "contextWindow": self.context_window,
            "priceInput": self.price_input,
            "priceOutput": self.price_output,
            "ttftMs": self.ttft_ms,
            "latencyMs": self.latency_ms,
            "codingScore": self.coding,
            "capabilities": self.capabilities,
        })
    }
}

#[derive(Clone, Debug)]
struct Entry {
    value: Option<f64>,
    source: String,
    confidence: f64,
    fetched_at: String,
}

impl Entry {
    fn fact(&self) -> Option<Fact> {
        self.value.map(|value| Fact {
            value,
            source: self.source.clone(),
            confidence: self.confidence,
            fetched_at: self.fetched_at.clone(),
        })
    }
}

/// Effective facts keyed by deployment id. Each target pairs a deployment id
/// with its canonical model id, when it has one.
pub fn facts_for(
    conn: &Connection,
    targets: &[(i64, Option<i64>)],
) -> rusqlite::Result<HashMap<i64, ModelFacts>> {
    let mut metrics: Vec<&str> = QUALITY_METRICS.to_vec();
    metrics.extend([
        SPEED_METRIC,
        CODING_METRIC,
        CONTEXT_METRIC,
        PRICE_INPUT_METRIC,
        PRICE_OUTPUT_METRIC,
        TTFT_METRIC,
        LATENCY_METRIC,
    ]);
    let placeholders = vec!["?"; metrics.len()].join(",");
    let sql = format!(
        "SELECT e.target_key, e.metric, json_extract(o.value_json,'$.value'), o.source, o.confidence, o.fetched_at \
         FROM model_metadata_effective e JOIN model_metadata_observations o ON o.id = e.observation_id \
         WHERE e.metric IN ({placeholders}) OR e.metric LIKE '{CAPABILITY_PREFIX}%'"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(metrics.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut by_target: HashMap<String, HashMap<String, Entry>> = HashMap::new();
    for row in rows {
        let (target, metric, value, source, confidence, fetched_at) = row?;
        by_target.entry(target).or_default().insert(
            metric,
            Entry {
                value,
                source,
                confidence,
                fetched_at,
            },
        );
    }

    let mut out = HashMap::with_capacity(targets.len());
    for &(id, canonical) in targets {
        let deployment = by_target.get(&format!("d:{id}"));
        let canonical = canonical.and_then(|c| by_target.get(&format!("c:{c}")));
        let lookup = |metric: &str| -> Option<&Entry> {
            deployment
                .and_then(|m| m.get(metric))
                .filter(|e| e.value.is_some())
                .or_else(|| {
                    canonical
                        .and_then(|m| m.get(metric))
                        .filter(|e| e.value.is_some())
                })
        };
        let number = |metric: &str| lookup(metric).and_then(|e| e.value);

        let mut capabilities: Vec<String> = [deployment, canonical]
            .into_iter()
            .flatten()
            .flat_map(|m| m.iter())
            .filter(|(metric, entry)| {
                metric.starts_with(CAPABILITY_PREFIX) && entry.value.is_some_and(|v| v >= 0.5)
            })
            .map(|(metric, _)| metric[CAPABILITY_PREFIX.len()..].to_string())
            .collect();
        capabilities.sort();
        capabilities.dedup();

        out.insert(
            id,
            ModelFacts {
                quality: QUALITY_METRICS
                    .iter()
                    .find_map(|metric| lookup(metric))
                    .and_then(Entry::fact),
                speed: lookup(SPEED_METRIC).and_then(Entry::fact),
                coding: number(CODING_METRIC),
                context_window: number(CONTEXT_METRIC),
                price_input: number(PRICE_INPUT_METRIC),
                price_output: number(PRICE_OUTPUT_METRIC),
                ttft_ms: number(TTFT_METRIC),
                latency_ms: number(LATENCY_METRIC),
                capabilities,
            },
        );
    }
    Ok(out)
}

/// Status of a metadata source (`available`, `notConfigured`, `never`, ...),
/// or `None` when the source is unknown.
pub fn source_status(conn: &Connection, source: &str) -> rusqlite::Result<Option<String>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT status FROM metadata_sources WHERE id = ?1",
        [source],
        |row| row.get(0),
    )
    .optional()
}
