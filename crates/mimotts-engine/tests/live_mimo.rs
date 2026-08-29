//! Live MiMo API integration tests (opt-in, real network).
//!
//! These tests hit the official MiMo-V2.5-TTS endpoint with a real API key.
//! They SKIP automatically when no key is configured, so `cargo test` stays
//! green in CI without secrets.
//!
//! Configuration (local env only, never committed):
//! - `MIMO_API_KEY`      — required to run. Read from the process env first,
//!   then from `<workspace>/.env.local` / `.env` (dotenvy never overrides an
//!   existing variable, so a CI-injected key always wins).
//! - `MIMOTTS_BASE_URL`  — optional upstream override; default is the official
//!   xiaomi endpoint.
//!
//! SECURITY: the key is never logged, never echoed into panics, and never
//! written to tracked files. `.env.local` is gitignored — keep it that way.

use std::path::Path;

use futures_util::StreamExt;

use mimotts_engine::mimo::{AudioChunk, MimoClient, SynthesisRequest, VoiceSpec};
use mimotts_engine::EngineError;

/// Official default upstream (matches the seeded xiaomi provider).
const DEFAULT_BASE_URL: &str = "https://api.xiaomimimo.com/v1";

/// Resolve the live API key without ever exposing its value in output.
/// Order: process env (CI) → `<workspace>/.env.local` → `<workspace>/.env`.
fn live_key() -> Option<String> {
    // Load local files first: dotenvy never overrides existing vars, so a
    // real process env still wins.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(workspace) = manifest.parent().and_then(|p| p.parent()) {
        for candidate in [workspace.join(".env.local"), workspace.join(".env")] {
            if candidate.is_file() {
                let _ = dotenvy::from_path(&candidate);
            }
        }
    }
    let _ = dotenvy::dotenv(); // fallback: package dir `.env`
    std::env::var("MIMO_API_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

fn live_base_url() -> String {
    std::env::var("MIMOTTS_BASE_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Skip marker: returns the key when live testing is configured, otherwise
/// prints one SKIP line and returns None (test returns early, stays green).
fn skip_when_unconfigured() -> Option<String> {
    match live_key() {
        Some(key) => Some(key),
        None => {
            eprintln!(
                "SKIP: MIMO_API_KEY not set — put it in .env.local (gitignored) \
                 or export it; no live request was made"
            );
            None
        }
    }
}

fn base_request(key: &str, text: &str, format: &str, stream: bool) -> SynthesisRequest {
    SynthesisRequest {
        model: "mimo-v2.5-tts".into(),
        style: Some("用平稳的语气，正常语速".into()),
        text: text.into(),
        voice: VoiceSpec::Preset("mimo_default".into()),
        format: format.into(),
        stream,
        optimize_text_preview: false,
        api_key: key.to_string(),
        base_url: live_base_url(),
    }
}

#[tokio::test]
async fn live_non_streaming_wav() {
    let Some(key) = skip_when_unconfigured() else {
        return;
    };
    let bytes = MimoClient::new()
        .synthesize_once(&base_request(&key, "你好，这是后端实网集成测试。", "wav", false))
        .await
        .expect("live wav synthesis should succeed");
    assert!(bytes.len() > 44, "wav too small: {} bytes", bytes.len());
    assert_eq!(&bytes[0..4], b"RIFF", "wav must start with RIFF");
    assert_eq!(&bytes[8..12], b"WAVE", "wav must carry WAVE tag");
    eprintln!("live wav ok: {} bytes", bytes.len()); // size only — never content
}

#[tokio::test]
async fn live_streaming_pcm16() {
    let Some(key) = skip_when_unconfigured() else {
        return;
    };
    let mut stream = MimoClient::new()
        .synthesize_stream(&base_request(&key, "这是一次流式后端集成测试。", "pcm16", true))
        .await
        .expect("live pcm16 stream should start");
    let mut pcm = Vec::new();
    let mut done = false;
    while let Some(item) = stream.next().await {
        match item.expect("live stream frame") {
            AudioChunk::Bytes(b) => pcm.extend_from_slice(&b),
            AudioChunk::Done => {
                done = true;
                break; // stream is consumed — stop polling
            }
        }
    }
    assert!(done, "stream must terminate with [DONE]");
    assert!(!pcm.is_empty(), "stream produced no audio bytes");
    assert_eq!(pcm.len() % 2, 0, "pcm16 stream must stay 16-bit aligned");
    // 24kHz mono 16-bit → 48000 bytes/sec
    eprintln!("live pcm16 ok: {} bytes (~{} ms)", pcm.len(), pcm.len() / 48);
}

#[tokio::test]
async fn live_invalid_key_is_rejected() {
    let Some(_key) = skip_when_unconfigured() else {
        return;
    };
    let req = base_request("sk-invalid-key-for-live-test", "测试。", "wav", false);
    let err = MimoClient::new()
        .synthesize_once(&req)
        .await
        .expect_err("an invalid key must be rejected by the official API");
    assert!(
        matches!(err, EngineError::Unauthorized(_)),
        "expected Unauthorized, got: {err:?}"
    );
    eprintln!("live auth ok: invalid key rejected as Unauthorized");
}
