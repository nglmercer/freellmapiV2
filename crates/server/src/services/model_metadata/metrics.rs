use super::types::{MetadataObservation, MetricValue};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDefinition {
    pub key: String,
    pub value_type: &'static str,
    pub unit: &'static str,
    pub direction: &'static str,
    pub cross_source_comparable: bool,
}
pub fn definition(key: &str) -> Option<MetricDefinition> {
    let (kind, unit, direction, comparable) = if key.starts_with("benchmark.aa.")
        || key.starts_with("benchmark.manual.")
    {
        ("number", "index", "higher", false)
    } else if key.starts_with("capability.") {
        ("boolean", "boolean", "none", true)
    } else {
        match key {
            "performance.output_tps" => ("number", "tokens/s", "higher", false),
            "performance.ttft_ms" | "performance.latency_ms" => ("number", "ms", "lower", false),
            "context.input" | "context.output" | "context.total" => {
                ("number", "tokens", "higher", true)
            }
            "price.input" | "price.output" | "price.cached_input" => {
                ("number", "USD/1M tokens", "lower", true)
            }
            "identity.name"
            | "identity.family"
            | "identity.publisher"
            | "identity.release_date"
            | "identity.description"
            | "identity.license"
            | "identity.huggingface"
            | "identity.architecture" => ("text", "text", "none", true),
            "identity.parameters" => ("number", "parameters", "none", true),
            "identity.open_weights" | "availability.free" => ("boolean", "boolean", "none", true),
            "modalities.input" | "modalities.output" | "parameters.supported" => {
                ("json", "json", "none", true)
            }
            _ => return None,
        }
    };
    Some(MetricDefinition {
        key: key.into(),
        value_type: kind,
        unit,
        direction,
        cross_source_comparable: comparable,
    })
}
pub fn validate(o: &MetadataObservation) -> bool {
    let Some(d) = definition(&o.metric) else {
        return false;
    };
    if o.unit != d.unit
        || !o.confidence.is_finite()
        || !(0.0..=1.0).contains(&o.confidence)
        || o.source_model_id.is_empty()
        || o.source_model_id.len() > 512
        || o.model_id.len() > 512
    {
        return false;
    }
    match &o.value {
        MetricValue::Number(n) => {
            d.value_type == "number"
                && n.is_finite()
                && *n >= 0.0
                && *n <= 1e15
                && (!o.metric.starts_with("context.") || n.fract() == 0.0)
        }
        MetricValue::Boolean(_) => d.value_type == "boolean",
        MetricValue::Text(s) => d.value_type == "text" && s.len() <= 8192,
        MetricValue::Json(v) => {
            d.value_type == "json" && v.is_array() && v.to_string().len() <= 4096
        }
    }
}
pub fn per_million(value: f64) -> Option<f64> {
    let n = value * 1_000_000.0;
    (value.is_finite() && value >= 0.0 && n.is_finite()).then_some(n)
}
#[cfg(test)]
mod tests {
    #[test]
    fn prices_are_not_per_token() {
        assert_eq!(super::per_million(0.000003), Some(3.0));
        assert_eq!(super::per_million(0.0), Some(0.0));
        assert_eq!(super::per_million(-1.0), None);
        assert_eq!(super::per_million(f64::NAN), None);
    }
}
