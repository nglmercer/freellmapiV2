//! Deterministic routing strategies built from quality and provider telemetry.

use std::collections::HashMap;

use serde::Serialize;

pub const QUALITY_WEIGHT: f64 = 0.45;
pub const RELIABILITY_WEIGHT: f64 = 0.25;
pub const SPEED_WEIGHT: f64 = 0.20;
pub const AVAILABILITY_WEIGHT: f64 = 0.10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingStrategy {
    Manual,
    Balanced,
    Quality,
    Fastest,
    MostReliable,
}

impl RoutingStrategy {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "balanced" => Some(Self::Balanced),
            "quality" | "intelligence" | "best-quality" => Some(Self::Quality),
            "fastest" | "speed" => Some(Self::Fastest),
            "reliability" | "most-reliable" => Some(Self::MostReliable),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
            Self::Fastest => "fastest",
            Self::MostReliable => "reliability",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoutingCandidate {
    pub model_db_id: i64,
    pub manual_priority: i64,
    pub quality: Option<f64>,
    pub speed: Option<f64>,
    pub reliability: Option<f64>,
    pub availability: f64,
    pub penalty: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredCandidate {
    pub model_db_id: i64,
    pub score: Option<f64>,
}

#[derive(Clone, Copy)]
struct Range {
    min: f64,
    max: f64,
}

fn range(values: impl Iterator<Item = Option<f64>>) -> Option<Range> {
    let mut range: Option<Range> = None;
    for value in values.flatten().filter(|value| value.is_finite()) {
        range = Some(match range {
            Some(current) => Range {
                min: current.min.min(value),
                max: current.max.max(value),
            },
            None => Range {
                min: value,
                max: value,
            },
        });
    }
    range
}

fn normalize(value: Option<f64>, bounds: Option<Range>) -> Option<f64> {
    let value = value?;
    let bounds = bounds?;
    if (bounds.max - bounds.min).abs() < f64::EPSILON {
        Some(1.0)
    } else {
        Some(((value - bounds.min) / (bounds.max - bounds.min)).clamp(0.0, 1.0))
    }
}

fn availability(candidate: &RoutingCandidate) -> f64 {
    (candidate.availability * (1.0 - candidate.penalty as f64 / 20.0)).clamp(0.0, 1.0)
}

fn weighted_score(
    candidate: &RoutingCandidate,
    quality_range: Option<Range>,
    speed_range: Option<Range>,
    reliability_range: Option<Range>,
) -> Option<f64> {
    let quality = normalize(candidate.quality, quality_range);
    let speed = normalize(candidate.speed, speed_range);
    let reliability = normalize(candidate.reliability, reliability_range);
    let availability = Some(availability(candidate));
    let dimensions = [
        (quality, QUALITY_WEIGHT),
        (reliability, RELIABILITY_WEIGHT),
        (speed, SPEED_WEIGHT),
        (availability, AVAILABILITY_WEIGHT),
    ];
    let total_weight: f64 = dimensions
        .iter()
        .filter_map(|(value, weight)| value.map(|_| *weight))
        .sum();
    if total_weight <= 0.0 {
        None
    } else {
        Some(
            dimensions
                .iter()
                .filter_map(|(value, weight)| value.map(|value| value * weight))
                .sum::<f64>()
                / total_weight,
        )
    }
}

fn focused_score(
    value: Option<f64>,
    bounds: Option<Range>,
    candidate: &RoutingCandidate,
) -> Option<f64> {
    // Availability can refine a focused metric, but it cannot substitute for
    // that metric. Otherwise an unmeasured model would outrank a measured
    // model with a poor (but real) score.
    let availability = availability(candidate);
    normalize(value, bounds).map(|value| value * 0.90 + availability * 0.10)
}

/// Calculate and deterministically order a set of route candidates. Missing
/// dimensions are omitted and the configured weights are renormalized.
pub fn score_candidates(
    candidates: &[RoutingCandidate],
    strategy: RoutingStrategy,
) -> Vec<ScoredCandidate> {
    // Build each cohort range exactly once. This keeps routing-score
    // calculation linear after the input scan instead of rebuilding/sorting a
    // dimension for every candidate.
    let quality_range = range(candidates.iter().map(|candidate| candidate.quality));
    let speed_range = range(candidates.iter().map(|candidate| candidate.speed));
    let reliability_range = range(candidates.iter().map(|candidate| candidate.reliability));
    let manual_priorities: HashMap<i64, i64> = candidates
        .iter()
        .map(|candidate| (candidate.model_db_id, candidate.manual_priority))
        .collect();
    let mut scored: Vec<ScoredCandidate> = candidates
        .iter()
        .map(|candidate| {
            let score = match strategy {
                RoutingStrategy::Manual => None,
                RoutingStrategy::Balanced => {
                    weighted_score(candidate, quality_range, speed_range, reliability_range)
                }
                RoutingStrategy::Quality => {
                    focused_score(candidate.quality, quality_range, candidate)
                }
                RoutingStrategy::Fastest => focused_score(candidate.speed, speed_range, candidate),
                RoutingStrategy::MostReliable => {
                    focused_score(candidate.reliability, reliability_range, candidate)
                }
            };
            ScoredCandidate {
                model_db_id: candidate.model_db_id,
                score,
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        let left = a.score.unwrap_or(f64::NEG_INFINITY);
        let right = b.score.unwrap_or(f64::NEG_INFINITY);
        right
            .total_cmp(&left)
            .then_with(|| {
                let ap = manual_priorities
                    .get(&a.model_db_id)
                    .copied()
                    .unwrap_or(i64::MAX);
                let bp = manual_priorities
                    .get(&b.model_db_id)
                    .copied()
                    .unwrap_or(i64::MAX);
                ap.cmp(&bp)
            })
            .then(a.model_db_id.cmp(&b.model_db_id))
    });
    scored
}

pub fn reliability_score(successes: i64, requests: i64) -> Option<f64> {
    if requests <= 0 {
        None
    } else {
        // Beta(8, 2) prior: one lucky request does not become 100% reliable.
        let successes = successes.clamp(0, requests);
        Some(((successes as f64) + 8.0) / ((requests as f64) + 10.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        id: i64,
        quality: Option<f64>,
        speed: Option<f64>,
        reliability: Option<f64>,
    ) -> RoutingCandidate {
        RoutingCandidate {
            model_db_id: id,
            manual_priority: id,
            quality,
            speed,
            reliability,
            availability: 1.0,
            penalty: 0,
        }
    }

    #[test]
    fn strategies_choose_expected_dimension() {
        let candidates = vec![
            candidate(1, Some(90.0), Some(20.0), Some(0.99)),
            candidate(2, Some(70.0), Some(200.0), Some(0.99)),
        ];
        assert_eq!(
            score_candidates(&candidates, RoutingStrategy::Quality)[0].model_db_id,
            1
        );
        assert_eq!(
            score_candidates(&candidates, RoutingStrategy::Fastest)[0].model_db_id,
            2
        );
    }

    #[test]
    fn missing_dimensions_are_renormalized() {
        let candidates = vec![
            candidate(1, Some(90.0), None, Some(0.9)),
            candidate(2, None, Some(200.0), Some(0.9)),
        ];
        let scored = score_candidates(&candidates, RoutingStrategy::Balanced);
        assert!(scored.iter().all(|candidate| candidate.score.is_some()));
    }

    #[test]
    fn focused_strategies_prioritize_real_metrics_over_missing_metrics() {
        let quality = score_candidates(
            &[
                candidate(1, Some(50.0), None, None),
                candidate(2, Some(100.0), None, None),
                candidate(3, None, None, None),
            ],
            RoutingStrategy::Quality,
        );
        assert_eq!(quality[0].model_db_id, 2);
        assert_eq!(quality[1].model_db_id, 1);
        assert_eq!(quality[2].model_db_id, 3);
        assert_eq!(quality[2].score, None);

        let fastest = score_candidates(
            &[
                candidate(1, None, Some(50.0), None),
                candidate(2, None, Some(100.0), None),
                candidate(3, None, None, None),
            ],
            RoutingStrategy::Fastest,
        );
        assert_eq!(fastest[1].model_db_id, 1);
        assert_eq!(fastest[2].model_db_id, 3);
        assert_eq!(fastest[2].score, None);

        let reliable = score_candidates(
            &[
                candidate(1, None, None, Some(0.5)),
                candidate(2, None, None, Some(1.0)),
                candidate(3, None, None, None),
            ],
            RoutingStrategy::MostReliable,
        );
        assert_eq!(reliable[1].model_db_id, 1);
        assert_eq!(reliable[2].model_db_id, 3);
        assert_eq!(reliable[2].score, None);
    }

    #[test]
    fn reliability_uses_a_prior() {
        assert_eq!(reliability_score(0, 0), None);
        assert!(reliability_score(1, 1).unwrap() < 1.0);
        assert!(reliability_score(99, 100).unwrap() > 0.9);
        assert!(reliability_score(2, 1).unwrap() <= 1.0);
    }
}
