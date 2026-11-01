//! Provider capability checks.
//!
//! The first test is offline: it verifies the declared capabilities of
//! built-in providers, so feature detection never depends on the network.
//!
//! The live tests make real calls with real keys. They are ignored by default
//! and also require `FREELLMAPI_LIVE_TESTS=1`, so CI never needs paid
//! credentials. Run them locally with:
//!
//! ```text
//! FREELLMAPI_LIVE_TESTS=1 GROQ_API_KEY=... \
//!   cargo test -p server --test provider_smoke -- --ignored
//! ```

use server::providers::{get_provider, Capability};

fn live_enabled() -> bool {
    std::env::var("FREELLMAPI_LIVE_TESTS").as_deref() == Ok("1")
}

#[tokio::test]
async fn declared_audio_capabilities_are_feature_detected_offline() {
    let groq = get_provider("groq").await.expect("groq is built in");
    let caps = groq.capabilities();
    assert!(caps.contains(Capability::Speech));
    assert!(caps.contains(Capability::Transcription));
    assert!(groq.speech().is_some());
    assert!(groq.transcription().is_some());
    // Not declared, so not claimed.
    assert!(groq.video().is_none());
    assert!(groq.images().is_none());
}

#[tokio::test]
async fn declared_embeddings_capability_is_feature_detected_offline() {
    let mistral = get_provider("mistral").await.expect("mistral is built in");
    assert!(mistral.capabilities().contains(Capability::Embeddings));
    assert!(mistral.embeddings().is_some());
    assert!(mistral.speech().is_none());
}

#[tokio::test]
#[ignore = "live: set FREELLMAPI_LIVE_TESTS=1 and GROQ_API_KEY"]
async fn live_groq_key_is_accepted() {
    if !live_enabled() {
        eprintln!("skipped: FREELLMAPI_LIVE_TESTS is not 1");
        return;
    }
    let Some(key) = std::env::var("GROQ_API_KEY").ok().filter(|k| !k.is_empty()) else {
        eprintln!("skipped: GROQ_API_KEY is not set");
        return;
    };
    let groq = get_provider("groq").await.expect("groq is built in");
    let accepted = groq.validate_key(&key).await.expect("transport ok");
    assert!(accepted, "Groq rejected the key");
}

#[tokio::test]
#[ignore = "live: set FREELLMAPI_LIVE_TESTS=1 and MISTRAL_API_KEY"]
async fn live_mistral_key_is_accepted() {
    if !live_enabled() {
        eprintln!("skipped: FREELLMAPI_LIVE_TESTS is not 1");
        return;
    }
    let Some(key) = std::env::var("MISTRAL_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
    else {
        eprintln!("skipped: MISTRAL_API_KEY is not set");
        return;
    };
    let mistral = get_provider("mistral").await.expect("mistral is built in");
    let accepted = mistral.validate_key(&key).await.expect("transport ok");
    assert!(accepted, "Mistral rejected the key");
}
