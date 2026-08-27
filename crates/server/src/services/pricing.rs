//! Model pricing lookup and cost calculation.

use rusqlite::Connection;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    pub prompt_cost: f64,
    pub completion_cost: f64,
    pub total_cost: f64,
    pub currency: String,
    pub pricing_prompt: Option<f64>,
    pub pricing_completion: Option<f64>,
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

pub fn calculate_cost(
    conn: &Connection,
    platform: &str,
    model_id: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
) -> CostEstimate {
    let row: Option<(Option<f64>, Option<f64>)> = conn
        .query_row(
            "SELECT pricing_prompt, pricing_completion FROM models \
             WHERE platform = ?1 AND model_id = ?2",
            rusqlite::params![platform, model_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();

    let pricing_prompt = row.and_then(|r| r.0);
    let pricing_completion = row.and_then(|r| r.1);

    if pricing_prompt.is_none() && pricing_completion.is_none() {
        return CostEstimate {
            prompt_cost: 0.0,
            completion_cost: 0.0,
            total_cost: 0.0,
            currency: "USD".to_string(),
            pricing_prompt: None,
            pricing_completion: None,
        };
    }

    let prompt_cost = prompt_tokens as f64 * pricing_prompt.unwrap_or(0.0);
    let completion_cost = completion_tokens as f64 * pricing_completion.unwrap_or(0.0);

    CostEstimate {
        prompt_cost: round6(prompt_cost),
        completion_cost: round6(completion_cost),
        total_cost: round6(prompt_cost + completion_cost),
        currency: "USD".to_string(),
        pricing_prompt,
        pricing_completion,
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PricingRow {
    pub platform: String,
    pub model_id: String,
    pub display_name: String,
    pub pricing_prompt: Option<f64>,
    pub pricing_completion: Option<f64>,
    pub free_tier: bool,
}

pub fn get_all_pricing(conn: &Connection) -> Vec<PricingRow> {
    let mut stmt = conn
        .prepare(
            "SELECT platform, model_id, display_name, pricing_prompt, pricing_completion, free_tier \
             FROM models WHERE enabled = 1",
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(PricingRow {
            platform: r.get(0)?,
            model_id: r.get(1)?,
            display_name: r.get(2)?,
            pricing_prompt: r.get(3)?,
            pricing_completion: r.get(4)?,
            free_tier: r.get::<_, i64>(5)? == 1,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}
