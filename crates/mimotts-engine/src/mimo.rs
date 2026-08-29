//! MimoClient v2 — official MiMo-V2.5-TTS contract.
//!
//! Spec (https://mimo.mi.com/docs/en-US/api/audio/tts):
//! - `POST {base}/v1/chat/completions`; auth `api-key` header (or Bearer)
//! - `user` message = style/pace/tone instructions (NOT spoken);
//!   `assistant` message = target text (+ inline tags)
//! - `audio { format: wav|mp3|pcm|pcm16, voice, optimize_text_preview }`
//! - streaming: `choices.delta.audio.data` base64 chunks; official guidance:
//!   use `pcm16` when streaming, splice chunks into one 24kHz mono PCM16LE.
//!
//! Error taxonomy per official error-codes page — 421 = content moderation
//! (never retry), 429 = over-frequency OR Token Plan exhausted, 400 includes
//! context/length overflow (→ ADR-013 re-chunk at ×0.8).

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::Value;

use crate::error::EngineError;

/// Voice spec: preset ID, or a voiceclone sample as base64 data-URI.
#[derive(Debug, Clone)]
pub enum VoiceSpec {
    Preset(String),
    /// `data:{mime};base64,{payload}` (mp3/wav sample, base64 ≤ 10MB).
    CloneDataUri(String),
}

impl VoiceSpec {
    pub fn clone_sample(mime: &str, sample_base64: &str) -> Self {
        Self::CloneDataUri(format!("data:{mime};base64,{sample_base64}"))
    }
}

#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    pub model: String,
    /// `user` message content (style instructions). Omitted when `None`.
    pub style: Option<String>,
    /// `assistant` message content — the text to speak (tags included).
    pub text: String,
    pub voice: VoiceSpec,
    /// `wav` | `mp3` | `pcm16`. Streaming forces `pcm16` per official guidance.
    pub format: String,
    pub stream: bool,
    pub optimize_text_preview: bool,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Debug)]
pub enum AudioChunk {
    /// Streaming delta bytes (raw pcm16 when format=pcm16).
    Bytes(Vec<u8>),
    /// Stream finished successfully.
    Done,
}

pub struct MimoClient {
    http: reqwest::Client,
}

impl MimoClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .pool_max_idle_per_host(64)
                .tcp_keepalive(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
        }
    }

    fn url(base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        }
    }

    fn body(req: &SynthesisRequest) -> serde_json::Value {
        let mut messages = Vec::new();
        // Official order: user first, assistant second.
        if let Some(style) = &req.style {
            if !style.trim().is_empty() {
                messages.push(serde_json::json!({"role": "user", "content": style}));
            }
        }
        if !req.optimize_text_preview || !req.text.trim().is_empty() {
            messages.push(serde_json::json!({"role": "assistant", "content": req.text}));
        }
        let voice = match &req.voice {
            VoiceSpec::Preset(id) => serde_json::json!(id),
            VoiceSpec::CloneDataUri(uri) => serde_json::json!(uri),
        };
        let mut audio = serde_json::json!({
            "format": req.format,
            "voice": voice,
        });
        if req.model == "mimo-v2.5-tts-voicedesign" {
            // voicedesign carries no preset voice (official spec) and is the
            // only model that accepts optimize_text_preview — the live API
            // rejects the field outright for other models
            // (400 Param Incorrect: "only supported for voice_design").
            audio = serde_json::json!({
                "format": req.format,
                "optimize_text_preview": req.optimize_text_preview,
            });
        }
        serde_json::json!({
            "model": req.model,
            "messages": messages,
            "audio": audio,
            "stream": req.stream,
        })
    }

    /// Classify an HTTP error per the official error-code semantics.
    fn classify(status: u16, body: &str) -> EngineError {
        match status {
            401 | 403 => EngineError::Unauthorized(format!("http {status}")),
            404 => EngineError::NotFound(format!("http {status}: {body}")),
            421 => EngineError::ContentBlocked,
            429 => EngineError::RateLimited,
            400 => {
                let lower = body.to_lowercase();
                // Narrow match: a bare "token" also appears in auth errors
                // ("invalid token") which must NOT trigger context re-chunk.
                if lower.contains("context")
                    || lower.contains("length")
                    || lower.contains("overflow")
                    || (lower.contains("token")
                        && (lower.contains("exceed") || lower.contains("limit")))
                    || body.contains("长度")
                    || body.contains("上下文")
                {
                    EngineError::ContextOverflow(body.to_string())
                } else {
                    EngineError::InvalidInput(format!("http 400: {body}"))
                }
            }
            500..=599 => EngineError::ServerOverload(format!("http {status}: {body}")),
            other => EngineError::Internal(format!("http {other}: {body}")),
        }
    }

    /// Non-streaming synthesis → full audio bytes (base64 decoded).
    pub async fn synthesize_once(&self, req: &SynthesisRequest) -> Result<Vec<u8>, EngineError> {
        let resp = self
            .http
            .post(Self::url(&req.base_url))
            .header("api-key", &req.api_key)
            .header("Content-Type", "application/json")
            .json(&Self::body(req))
            .send()
            .await
            .map_err(|e| {
                // Any send-stage failure is transport (connect refused/reset,
                // timeout, hyper error) → transient, retryable. Build errors
                // can't surface here: they fail before `.send()`.
                EngineError::ServerOverload(format!("transport: {e}"))
            })?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::classify(status, &body));
        }
        let json: Value = resp.json().await?;
        let b64 = json
            .pointer("/choices/0/message/audio/data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EngineError::Internal("no audio data in response".into()))?;
        B64.decode(b64)
            .map_err(|e| EngineError::Internal(format!("base64 decode: {e}")))
    }

    /// Streaming synthesis → `AudioStream` yielding raw audio bytes.
    pub async fn synthesize_stream(
        &self,
        req: &SynthesisRequest,
    ) -> Result<AudioStream, EngineError> {
        let resp = self
            .http
            .post(Self::url(&req.base_url))
            .header("api-key", &req.api_key)
            .header("Content-Type", "application/json")
            .json(&Self::body(req))
            .send()
            .await
            .map_err(|e| {
                // Send-stage transport failure → transient, retryable.
                EngineError::ServerOverload(format!("transport: {e}"))
            })?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::classify(status, &body));
        }
        let stream = resp.bytes_stream();
        let decoder = SseDecoder::default();
        Ok(AudioStream {
            inner: Box::pin(stream),
            decoder,
            pending: VecDeque::new(),
            done: false,
        })
    }
}

/// Minimal SSE frame decoder (only cares about `data:` lines).
/// The buffer is capped: a malformed upstream that never emits a frame
/// separator must not grow memory without bound.
#[derive(Default)]
struct SseDecoder {
    buf: String,
}

/// 1 MiB is far above any legitimate SSE frame (audio b64 chunks are ~KB).
const MAX_SSE_BUFFER: usize = 1024 * 1024;

impl SseDecoder {
    /// Feed raw bytes; returns completed `data:` payloads.
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, EngineError> {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        if self.buf.len() > MAX_SSE_BUFFER {
            return Err(EngineError::Internal("sse frame buffer overflow".into()));
        }
        let mut out = Vec::new();
        while let Some(idx) = self.buf.find("\n\n") {
            let frame = self.buf[..idx].to_string();
            self.buf.drain(..idx + 2);
            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if !data.is_empty() {
                        out.push(data.to_string());
                    }
                }
            }
        }
        // tolerate \r\n\r\n
        while let Some(idx) = self.buf.find("\r\n\r\n") {
            let frame = self.buf[..idx].to_string();
            self.buf.drain(..idx + 4);
            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if !data.is_empty() {
                        out.push(data.to_string());
                    }
                }
            }
        }
        Ok(out)
    }
}

#[derive(Default, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

#[derive(Default, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Default, Deserialize)]
struct StreamDelta {
    audio: Option<AudioDelta>,
}

#[derive(Deserialize)]
struct AudioDelta {
    data: Option<String>,
}

pub struct AudioStream {
    inner: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    decoder: SseDecoder,
    /// Frames parsed but not yet yielded (a single network chunk may carry
    /// several SSE frames — never drop them).
    pending: VecDeque<AudioChunk>,
    /// Set once `[DONE]` was parsed: subsequent polls must return `None`,
    /// not "stream ended without [DONE]" (the inner stream is already EOF).
    done: bool,
}

impl Stream for AudioStream {
    type Item = Result<AudioChunk, EngineError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(chunk) = self.pending.pop_front() {
                return Poll::Ready(Some(Ok(chunk)));
            }
            if self.done {
                return Poll::Ready(None);
            }
            let item = futures_util::Stream::poll_next(self.inner.as_mut(), cx);
            match item {
                Poll::Ready(Some(Ok(bytes))) => {
                    let frames = match self.decoder.push(&bytes) {
                        Ok(f) => f,
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    };
                    for frame in frames {
                        if frame == "[DONE]" {
                            self.done = true;
                            self.pending.push_back(AudioChunk::Done);
                            continue;
                        }
                        let parsed: StreamChunk = match serde_json::from_str(&frame) {
                            Ok(p) => p,
                            Err(e) => {
                                return Poll::Ready(Some(Err(EngineError::Internal(format!(
                                    "sse json: {e}"
                                )))))
                            }
                        };
                        if let Some(b64) = parsed
                            .choices
                            .first()
                            .and_then(|c| c.delta.audio.as_ref())
                            .and_then(|a| a.data.as_ref())
                        {
                            match B64.decode(b64) {
                                Ok(bytes) => self.pending.push_back(AudioChunk::Bytes(bytes)),
                                Err(e) => {
                                    return Poll::Ready(Some(Err(EngineError::Internal(format!(
                                        "b64: {e}"
                                    )))))
                                }
                            }
                        }
                        // empty choices (usage-only frame) → skip
                    }
                    // loop back and drain pending
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(EngineError::Internal(format!("stream: {e}")))));
                }
                Poll::Ready(None) => {
                    if self.done {
                        return Poll::Ready(None);
                    }
                    if self.pending.is_empty() {
                        return Poll::Ready(Some(Err(EngineError::Internal(
                            "stream ended without [DONE]".into(),
                        ))));
                    }
                    // drain whatever we have, then error on next poll
                    if let Some(chunk) = self.pending.pop_front() {
                        return Poll::Ready(Some(Ok(chunk)));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn req(server: &MockServer) -> SynthesisRequest {
        SynthesisRequest {
            model: "mimo-v2.5-tts".into(),
            style: Some("用温柔的语气，语速稍慢".into()),
            text: "你好，世界。".into(),
            voice: VoiceSpec::Preset("冰糖".into()),
            format: "wav".into(),
            stream: false,
            optimize_text_preview: false,
            api_key: "test-key".into(),
            base_url: server.uri(),
        }
    }

    #[tokio::test]
    async fn request_shape_matches_official_contract() {
        let server = MockServer::start().await;
        let wav = B64.encode(b"RIFFfake");
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"audio": {"data": wav, "transcript": null}}}]
            })))
            .mount(&server)
            .await;

        let client = MimoClient::new();
        let mut r = req(&server);
        let bytes = client.synthesize_once(&r).await.unwrap();
        assert_eq!(bytes, b"RIFFfake");

        // Verify the actual request body fields.
        let received = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&received[0].body).unwrap();
        assert_eq!(body["model"], "mimo-v2.5-tts");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "用温柔的语气，语速稍慢");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["content"], "你好，世界。");
        assert_eq!(body["audio"]["format"], "wav");
        assert_eq!(body["audio"]["voice"], "冰糖");
        assert!(
            body["audio"].get("optimize_text_preview").is_none(),
            "optimize_text_preview is voicedesign-only; the live API 400s otherwise"
        );
        assert_eq!(body["stream"], false);
        assert_eq!(
            received[0].headers.get("api-key").unwrap(),
            "test-key",
            "api-key header per official auth"
        );
        r.style = None;
        let body2 = MimoClient::body(&r);
        assert_eq!(body2["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body2["messages"][0]["role"], "assistant");
    }

    #[test]
    fn voicedesign_audio_shape_swaps_voice_for_preview() {
        let mut r = SynthesisRequest {
            model: "mimo-v2.5-tts-voicedesign".into(),
            style: Some("温和".into()),
            text: "你好".into(),
            voice: VoiceSpec::Preset("冰糖".into()),
            format: "wav".into(),
            stream: false,
            optimize_text_preview: true,
            api_key: "k".into(),
            base_url: "http://localhost".into(),
        };
        let body = MimoClient::body(&r);
        assert_eq!(body["audio"]["optimize_text_preview"], true);
        assert!(
            body["audio"].get("voice").is_none(),
            "voicedesign must not carry a preset voice"
        );
        // Base model must NOT send the field at all (live API 400s on it).
        r.model = "mimo-v2.5-tts".into();
        r.optimize_text_preview = false;
        let base = MimoClient::body(&r);
        assert!(base["audio"].get("optimize_text_preview").is_none());
        assert_eq!(base["audio"]["voice"], "冰糖");
    }

    #[tokio::test]
    async fn classifies_429_421_400_context() {
        for (status, body, expect) in [
            (429, "rate limit", "rate_limited"),
            (421, "blocked", "content_blocked"),
            (400, "context length exceeded", "context_overflow"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(ResponseTemplate::new(status).set_body_string(body.to_string()))
                .mount(&server)
                .await;
            let client = MimoClient::new();
            match client.synthesize_once(&req(&server)).await {
                Err(e) => match expect {
                    "rate_limited" => assert!(matches!(e, EngineError::RateLimited), "{e:?}"),
                    "content_blocked" => assert!(matches!(e, EngineError::ContentBlocked), "{e:?}"),
                    "context_overflow" => {
                        assert!(matches!(e, EngineError::ContextOverflow(_)), "{e:?}")
                    }
                    _ => unreachable!(),
                },
                Ok(_) => panic!("status {status} should error"),
            }
        }
    }

    #[tokio::test]
    async fn streaming_decodes_deltas() {
        let server = MockServer::start().await;
        let b1 = B64.encode(b"\x01\x02\x03");
        let b2 = B64.encode(b"\x04\x05\x06");
        let body = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"audio\":{{\"data\":\"{b1}\"}}}}}}]}}\n\n\
             data: {{\"choices\":[{{\"delta\":{{\"audio\":{{\"data\":\"{b2}\"}}}}}}]}}\n\n\
             data: {{\"choices\":[]}}\n\n\
             data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let client = MimoClient::new();
        let mut r = req(&server);
        r.stream = true;
        r.format = "pcm16".into();
        let mut stream = client.synthesize_stream(&r).await.unwrap();
        use futures_util::StreamExt;
        let mut collected = Vec::new();
        let mut done = false;
        while let Some(item) = stream.next().await {
            match item.unwrap() {
                AudioChunk::Bytes(b) => collected.extend_from_slice(&b),
                AudioChunk::Done => {
                    done = true;
                    break; // stream is consumed — stop polling
                }
            }
        }
        assert!(done);
        assert_eq!(collected, vec![1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn streaming_429_fails_fast() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        let client = MimoClient::new();
        let mut r = req(&server);
        r.stream = true;
        match client.synthesize_stream(&r).await {
            Err(EngineError::RateLimited) => {}
            Err(other) => panic!("expected RateLimited, got: {other:?}"),
            Ok(_) => panic!("expected error, got a stream"),
        }
    }

    #[tokio::test]
    async fn voiceclone_data_uri_in_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"audio": {"data": B64.encode(b"wav")}}}]
            })))
            .mount(&server)
            .await;
        let mut r = req(&server);
        r.model = "mimo-v2.5-tts-voiceclone".into();
        r.voice = VoiceSpec::clone_sample("audio/wav", B64.encode(b"sample").as_str());
        let _ = MimoClient::new().synthesize_once(&r).await.unwrap();
        let received = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&received[0].body).unwrap();
        let voice = body["audio"]["voice"].as_str().unwrap();
        assert!(voice.starts_with("data:audio/wav;base64,"), "{voice}");
    }
}
