//! Hard eligibility and deployment pricing are shared by simulation and live fallback.
use super::persistence;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingPolicy {
    pub strategy: Option<String>,
    pub required_capabilities: Vec<String>,
    pub allowed_providers: Vec<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub free_only: bool,
    pub max_cost_usd: Option<f64>,
    pub weights: Option<Weights>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Weights {
    pub quality: f64,
    pub speed: f64,
    pub reliability: f64,
    pub latency: f64,
    pub price: f64,
    pub availability: f64,
}
impl Default for Weights {
    fn default() -> Self {
        Self {
            quality: 0.35,
            speed: 0.15,
            reliability: 0.2,
            latency: 0.1,
            price: 0.1,
            availability: 0.1,
        }
    }
}
impl RoutingPolicy {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.input_tokens < 0
            || self.output_tokens < 0
            || self.input_tokens > 100_000_000
            || self.output_tokens > 100_000_000
            || self.allowed_providers.len() > 100
            || self.required_capabilities.len() > 20
            || self.max_cost_usd.is_some_and(|n| !n.is_finite() || n < 0.0)
        {
            return Err("Invalid routing limits");
        }
        if self.required_capabilities.iter().any(|s| {
            ![
                "tools",
                "vision",
                "structured_output",
                "reasoning",
                "audio_input",
                "video_input",
                "embeddings",
                "streaming",
            ]
            .contains(&s.as_str())
        }) {
            return Err("Invalid required capability");
        }
        if self.strategy.as_deref().is_some_and(|s| {
            ![
                "manual",
                "balanced",
                "quality",
                "fastest",
                "most-reliable",
                "reliability",
                "lowest-latency",
                "cheapest",
                "best-coding",
                "best-reasoning",
                "free-only",
                "weighted",
            ]
            .contains(&s)
        }) {
            return Err("Invalid routing strategy");
        }
        if let Some(w) = &self.weights {
            let values = [
                w.quality,
                w.speed,
                w.reliability,
                w.latency,
                w.price,
                w.availability,
            ];
            if values
                .iter()
                .any(|n| !n.is_finite() || *n < 0.0 || *n > 1.0)
                || values.iter().sum::<f64>() <= 0.0
            {
                return Err("Invalid routing weights");
            }
        }
        Ok(())
    }
    pub fn from_request(raw: &Value, input: i64, output: i64) -> Self {
        let mut p = Self {
            input_tokens: input.max(0),
            output_tokens: output.max(0),
            ..Default::default()
        };
        if raw
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
        {
            p.required_capabilities.push("tools".into());
        }
        if raw.pointer("/response_format/type").and_then(Value::as_str) == Some("json_schema") {
            p.required_capabilities.push("structured_output".into());
        }
        if let Some(messages) = raw.get("messages").and_then(Value::as_array) {
            for message in messages {
                if let Some(parts) = message.get("content").and_then(Value::as_array) {
                    for part in parts {
                        let cap = match part.get("type").and_then(Value::as_str) {
                            Some("image_url") => Some("vision"),
                            Some("input_audio") => Some("audio_input"),
                            Some("video_url") => Some("video_input"),
                            _ => None,
                        };
                        if let Some(cap) = cap {
                            if !p.required_capabilities.iter().any(|s| s == cap) {
                                p.required_capabilities.push(cap.into());
                            }
                        }
                    }
                }
            }
        }
        p
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub model_db_id: i64,
    pub eligible: bool,
    pub rejection_reasons: Vec<String>,
    pub strategy: String,
    pub score: Option<f64>,
    pub estimated_cost_usd: Option<f64>,
    pub metrics: Value,
}
fn metric<'a>(metrics: &'a Value, key: &str, scope: &str) -> Option<&'a Value> {
    metrics
        .as_array()?
        .iter()
        .find(|v| v["metric"] == key && v["scope"] == scope && v["freshness"] == "fresh")
}
fn number(metrics: &Value, key: &str, scope: &str) -> Option<f64> {
    metric(metrics, key, scope)?
        .pointer("/value/value")?
        .as_f64()
}
fn normalized(n: Option<f64>, scale: f64) -> Option<f64> {
    n.filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n / (n + scale))
}
pub fn evaluate(conn: &Connection, id: i64, p: &RoutingPolicy) -> rusqlite::Result<Decision> {
    let (platform, model, enabled, free, context, source): (
        String,
        String,
        bool,
        bool,
        Option<i64>,
        String,
    ) = conn.query_row(
        "SELECT platform,model_id,enabled,free_tier,context_window,source FROM models WHERE id=?1",
        [id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        },
    )?;
    let metrics = persistence::metrics_for(conn, id)?;
    let mut reasons = Vec::new();
    let strategy = p
        .strategy
        .clone()
        .or_else(|| {
            conn.query_row(
                "SELECT value FROM settings WHERE key='fallback_strategy'",
                [],
                |r| r.get(0),
            )
            .ok()
        })
        .unwrap_or_else(|| "manual".into());
    if !enabled {
        reasons.push("disabled".into());
    }
    let fallback: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM fallback_config WHERE model_db_id=?1 AND enabled=1)",
        [id],
        |r| r.get(0),
    )?;
    if !fallback {
        reasons.push("fallbackDisabled".into());
    }
    if crate::providers::get_provider_with_conn(conn, &platform).is_none() {
        reasons.push("providerUnavailable".into());
    }
    let available = crate::services::router::route_availability(conn, &platform, &model);
    if available <= 0.0 {
        reasons.push("credentialsOrCooldown".into());
    }
    let limits = conn.query_row(
        "SELECT rpm_limit,rpd_limit,tpm_limit,tpd_limit FROM models WHERE id=?1",
        [id],
        |r| {
            Ok(crate::services::ratelimit::Limits {
                rpm: r.get(0)?,
                rpd: r.get(1)?,
                tpm: r.get(2)?,
                tpd: r.get(3)?,
            })
        },
    )?;
    let key_ids = {
        let mut stmt=conn.prepare("SELECT id FROM api_keys WHERE platform=?1 AND enabled=1 AND status NOT IN ('invalid','rate_limited')")?;
        let ids = stmt
            .query_map([&platform], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids
    };
    if !key_ids.is_empty()
        && !key_ids.iter().any(|key| {
            !crate::services::ratelimit::is_on_cooldown(&platform, &model, *key)
                && crate::services::ratelimit::can_make_request(&platform, &model, *key, &limits)
                && crate::services::ratelimit::can_use_tokens(
                    &platform,
                    &model,
                    *key,
                    p.input_tokens.saturating_add(p.output_tokens),
                    (limits.tpm, limits.tpd),
                )
        })
    {
        reasons.push("quotaOrRateLimit".into());
    }
    if !p.allowed_providers.is_empty() && !p.allowed_providers.contains(&platform) {
        reasons.push("providerNotAllowed".into());
    }
    for capability in &p.required_capabilities {
        if metric(
            &metrics,
            &format!("capability.{capability}"),
            "providerDeployment",
        )
        .and_then(|m| m.pointer("/value/value"))
        .and_then(Value::as_bool)
            != Some(true)
        {
            reasons.push(format!("capability:{capability}:unsupportedOrUnknown"));
        }
    }
    let effective_context = number(&metrics, "context.total", "providerDeployment")
        .or_else(|| context.map(|n| n as f64));
    if (p.input_tokens > 0 || p.output_tokens > 0)
        && effective_context
            .is_none_or(|c| c < p.input_tokens.saturating_add(p.output_tokens) as f64)
    {
        reasons.push("insufficientOrUnknownContext".into());
    }
    if number(&metrics, "context.output", "providerDeployment")
        .is_some_and(|c| c < p.output_tokens as f64)
    {
        reasons.push("insufficientOutputLimit".into());
    }
    let input_price = number(&metrics, "price.input", "providerDeployment");
    let output_price = number(&metrics, "price.output", "providerDeployment");
    // Legacy price columns are USD/token. Use them only for explicitly manual records.
    let legacy: Option<(Option<f64>, Option<f64>)> = if source == "manual" {
        conn.query_row(
            "SELECT pricing_prompt,pricing_completion FROM models WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    } else {
        None
    };
    let input_price = input_price.or_else(|| legacy.and_then(|v| v.0).map(|v| v * 1e6));
    let output_price = output_price.or_else(|| legacy.and_then(|v| v.1).map(|v| v * 1e6));
    let cost = input_price
        .zip(output_price)
        .map(|(i, o)| (i * p.input_tokens as f64 + o * p.output_tokens as f64) / 1e6);
    let metadata_free = metric(&metrics, "availability.free", "providerDeployment")
        .and_then(|m| m.pointer("/value/value"))
        .and_then(Value::as_bool);
    if p.free_only || strategy == "free-only" {
        let known_free = metadata_free.unwrap_or(free);
        if !known_free
            || input_price.is_some_and(|n| n > 0.0)
            || output_price.is_some_and(|n| n > 0.0)
        {
            reasons.push("freeOnly".into());
        }
    }
    if let Some(max) = p.max_cost_usd {
        if cost.is_none_or(|v| v > max) {
            reasons.push("costLimitOrUnknownPrice".into());
        }
    }
    let performance = crate::services::telemetry::load_performance(conn, &platform, &model)?;
    let sampled = performance.as_ref().filter(|v| {
        v.sample_count >= 5
            && chrono::DateTime::parse_from_rfc3339(&v.updated_at).is_ok_and(|d| {
                (chrono::Utc::now() - d.with_timezone(&chrono::Utc)).num_hours() < 24
            })
    });
    let legacy_quality:Option<(f64,Option<String>,Option<String>)>=conn.query_row("SELECT intelligence_score,quality_source,quality_updated_at FROM models WHERE id=?1 AND intelligence_score IS NOT NULL",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).ok();
    let quality = normalized(
        number(&metrics, "benchmark.manual.intelligence", "canonicalModel")
            .or_else(|| number(&metrics, "benchmark.aa.intelligence", "canonicalModel"))
            .or_else(|| {
                legacy_quality.as_ref().and_then(|(value, source, date)| {
                    let fresh = source.as_deref() == Some("manual")
                        || date
                            .as_ref()
                            .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                            .is_some_and(|d| {
                                (chrono::Utc::now() - d.with_timezone(&chrono::Utc)).num_hours()
                                    < 24
                            });
                    fresh.then_some(*value)
                })
            }),
        50.0,
    );
    let coding = normalized(
        number(&metrics, "benchmark.aa.coding", "canonicalModel"),
        50.0,
    );
    // A dedicated reasoning benchmark is required; do not relabel intelligence/agentic.
    let reasoning = normalized(
        number(&metrics, "benchmark.aa.reasoning", "canonicalModel"),
        50.0,
    );
    let speed = normalized(sampled.and_then(|v| v.ewma_output_tps), 100.0);
    let latency = sampled
        .and_then(|v| v.ewma_ttft_ms.or(v.ewma_latency_ms))
        .map(|v| 1000.0 / (1000.0 + v));
    let reliability = sampled
        .and_then(|v| crate::services::telemetry::reliability(v.success_count, v.sample_count));
    let cheap = cost.map(|v| 0.001 / (0.001 + v));
    let score = match strategy.as_str() {
        "manual" => None,
        "quality" => quality,
        "fastest" => speed,
        "most-reliable" | "reliability" => reliability,
        "lowest-latency" => latency,
        "cheapest" => cheap,
        "best-coding" => coding,
        "best-reasoning" => reasoning,
        _ => {
            let w = p.weights.clone().unwrap_or_default();
            let dimensions = [
                (quality, w.quality),
                (speed, w.speed),
                (reliability, w.reliability),
                (latency, w.latency),
                (cheap, w.price),
            ];
            if dimensions.iter().filter(|(v, _)| v.is_some()).count() < 2 {
                None
            } else {
                let total =
                    w.quality + w.speed + w.reliability + w.latency + w.price + w.availability;
                Some(
                    (dimensions
                        .iter()
                        .map(|(v, w)| v.unwrap_or(0.0) * w)
                        .sum::<f64>()
                        + available * w.availability)
                        / total,
                )
            }
        }
    };
    Ok(Decision {
        model_db_id: id,
        eligible: reasons.is_empty(),
        rejection_reasons: reasons,
        strategy,
        score,
        estimated_cost_usd: cost,
        metrics: json!({"provenance":metrics,"legacyQualityEvidence":legacy_quality,"localSampleCount":performance.map(|v|v.sample_count).unwrap_or(0),"quality":quality,"speed":speed,"latency":latency,"reliability":reliability}),
    })
}

pub fn configured_policy(conn: &Connection, request: &RoutingPolicy) -> RoutingPolicy {
    let mut base: RoutingPolicy = conn
        .query_row(
            "SELECT value FROM settings WHERE key='routing_policy'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| serde_json::from_str(&v).ok())
        .filter(|p: &RoutingPolicy| p.validate().is_ok())
        .unwrap_or_default();
    base.input_tokens = request.input_tokens;
    base.output_tokens = request.output_tokens;
    base.free_only |= request.free_only;
    if request.strategy.is_some() {
        base.strategy = request.strategy.clone();
    }
    if request.weights.is_some() {
        base.weights = request.weights.clone();
    }
    base.max_cost_usd = match (base.max_cost_usd, request.max_cost_usd) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    for cap in &request.required_capabilities {
        if !base.required_capabilities.contains(cap) {
            base.required_capabilities.push(cap.clone());
        }
    }
    if !request.allowed_providers.is_empty() {
        if base.allowed_providers.is_empty() {
            base.allowed_providers = request.allowed_providers.clone();
        } else {
            base.allowed_providers
                .retain(|v| request.allowed_providers.contains(v));
            if base.allowed_providers.is_empty() {
                base.allowed_providers.push("__none__".into());
            }
        }
    }
    base
}
