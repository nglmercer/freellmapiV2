//! Provider/model telemetry integration coverage.

mod common;

use server::services::telemetry::{record_observation, ObservationOutcome, RuntimeObservation};

#[tokio::test]
async fn observations_update_ewma_and_invalidate_stale_speed_rank() {
    let _app = common::setup().await;

    record_observation(
        "google",
        "gemini-2.5-pro",
        RuntimeObservation {
            outcome: ObservationOutcome::Success,
            latency_ms: Some(2_000),
            ttft_ms: Some(500),
            output_tokens: Some(100),
            generation_duration_ms: Some(1_000),
        },
    )
    .await
    .expect("first telemetry sample");
    record_observation(
        "google",
        "gemini-2.5-pro",
        RuntimeObservation {
            outcome: ObservationOutcome::Success,
            latency_ms: Some(1_000),
            ttft_ms: None,
            output_tokens: Some(50),
            generation_duration_ms: Some(1_000),
        },
    )
    .await
    .expect("second telemetry sample");
    record_observation(
        "google",
        "gemini-2.5-pro",
        RuntimeObservation {
            outcome: ObservationOutcome::Failure {
                status_code: Some(429),
                message: "rate limited".to_string(),
            },
            latency_ms: Some(100),
            ttft_ms: None,
            output_tokens: None,
            generation_duration_ms: None,
        },
    )
    .await
    .expect("failure telemetry sample");

    let conn = server::db::db().lock().await;
    let performance: (i64, i64, i64, i64, f64) = conn
        .query_row(
            "SELECT sample_count, success_count, failure_count, rate_limit_count,
                    ewma_output_tps
             FROM model_performance
             WHERE platform = 'google' AND model_id = 'gemini-2.5-pro'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("performance row");
    assert_eq!(performance.0, 3);
    assert_eq!(performance.1, 2);
    assert_eq!(performance.2, 1);
    assert_eq!(performance.3, 1);
    assert!((performance.4 - 90.0).abs() < f64::EPSILON);

    let model: (i64, f64, String) = conn
        .query_row(
            "SELECT speed_rank, observed_speed_tps, speed_source
             FROM models WHERE platform = 'google' AND model_id = 'gemini-2.5-pro'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("updated model row");
    assert_eq!(model.0, 0);
    assert!((model.1 - 50.0).abs() < f64::EPSILON);
    assert_eq!(model.2, "local-observed");
}
