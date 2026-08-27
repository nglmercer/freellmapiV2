//! Probe every enabled model that has an enabled API key.

use std::time::Instant;

use server::crypto;
use server::db;
use server::env;
use server::types::{ChatMessage, CompletionOptions};

#[derive(Debug, Clone)]
struct ModelTarget {
    platform: String,
    model_id: String,
}

#[derive(Debug)]
struct ProbeResult {
    target: ModelTarget,
    ok: bool,
    elapsed_ms: u128,
    detail: String,
}

#[tokio::main]
async fn main() {
    env::init();
    server::logger::init();

    db::init_db(None).expect("failed to initialize database");
    db::run_in_transaction(|conn| {
        db::run_migrations(conn);
        Ok::<(), rusqlite::Error>(())
    })
    .await
    .expect("failed to run database migrations");

    let targets = load_targets().await;
    let mut results = Vec::with_capacity(targets.len());

    for target in targets {
        results.push(probe_model(target).await);
    }

    println!("\n=== Results ===\n");
    for result in &results {
        let status = if result.ok { '✓' } else { '✗' };
        println!(
            "{status} {} {} {:>5}ms  {}",
            pad(status_text(&result.target.platform, 12), 12),
            pad(status_text(&result.target.model_id, 52), 52),
            result.elapsed_ms,
            result.detail
        );
    }

    let working = results.iter().filter(|result| result.ok).count();
    println!("\n{working}/{} models working\n", results.len());
}

async fn load_targets() -> Vec<ModelTarget> {
    let conn = db::db().lock().await;
    let mut statement = conn
        .prepare(
            "SELECT m.platform, m.model_id
             FROM models AS m
             WHERE m.enabled = 1
               AND EXISTS (
                   SELECT 1 FROM api_keys AS k
                   WHERE k.platform = m.platform AND k.enabled = 1
               )
             ORDER BY m.intelligence_rank ASC, m.platform ASC",
        )
        .expect("failed to query enabled models");

    statement
        .query_map([], |row| {
            Ok(ModelTarget {
                platform: row.get(0)?,
                model_id: row.get(1)?,
            })
        })
        .expect("failed to read enabled models")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("failed to collect enabled models")
}

async fn load_key(target: &ModelTarget) -> Option<(String, String, String)> {
    let conn = db::db().lock().await;
    conn.query_row(
        "SELECT encrypted_key, iv, auth_tag
         FROM api_keys
         WHERE platform = ?1 AND enabled = 1
         ORDER BY id ASC
         LIMIT 1",
        rusqlite::params![target.platform],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .ok()
}

async fn probe_model(target: ModelTarget) -> ProbeResult {
    let Some((encrypted_key, iv, auth_tag)) = load_key(&target).await else {
        return ProbeResult {
            target,
            ok: false,
            elapsed_ms: 0,
            detail: "no key".to_string(),
        };
    };

    let api_key = match crypto::decrypt(&encrypted_key, &iv, &auth_tag) {
        Ok(key) => key,
        Err(error) => {
            return ProbeResult {
                target,
                ok: false,
                elapsed_ms: 0,
                detail: format!("decrypt failed: {error}"),
            };
        }
    };

    let Some(provider) = server::providers::get_provider(&target.platform).await else {
        return ProbeResult {
            target,
            ok: false,
            elapsed_ms: 0,
            detail: "no provider".to_string(),
        };
    };

    let started = Instant::now();
    let response = provider
        .chat_completion(
            &api_key,
            &[ChatMessage::text("user", "hi")],
            &target.model_id,
            &CompletionOptions {
                max_tokens: Some(5),
                ..CompletionOptions::default()
            },
        )
        .await;
    let elapsed_ms = started.elapsed().as_millis();

    match response {
        Ok(response) => {
            let reply = response
                .choices
                .first()
                .and_then(|choice| choice.message.content.as_text())
                .map(one_line)
                .unwrap_or_default();
            ProbeResult {
                target,
                ok: true,
                elapsed_ms,
                detail: format!("\"{}\"", truncate(&reply, 40)),
            }
        }
        Err(error) => ProbeResult {
            target,
            ok: false,
            elapsed_ms,
            detail: truncate(&one_line(&error.to_string()), 200),
        },
    }
}

fn status_text(value: &str, width: usize) -> String {
    truncate(value, width)
}

fn pad(value: String, width: usize) -> String {
    let len = value.chars().count();
    if len >= width {
        value
    } else {
        format!("{value}{}", " ".repeat(width - len))
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let mut shortened: String = truncated
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect();
        shortened.push('…');
        shortened
    } else {
        truncated
    }
}

fn one_line(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            other => other,
        })
        .collect()
}
