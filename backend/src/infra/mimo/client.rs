use base64::Engine;
use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// HTTP client for the MIMO TTS API.
///
/// Uses the `/v1/chat/completions` endpoint with `audio` modality,
/// which is the correct API format for Xiaomi MiMo TTS.
pub struct MimoClient {
    http_client: reqwest::Client,
    api_key: String,
    base_url: String,
}

// ── Request / Response types for MIMO chat completions TTS ──────────

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct AudioParams {
    format: String,
    voice: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    audio: AudioParams,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct AudioData {
    data: String,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    audio: Option<AudioData>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

impl MimoClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|e| {
                    tracing::error!("Failed to create HTTP client: {}, using default", e);
                    reqwest::Client::new()
                }),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Call the tokenize endpoint for general use.
    pub async fn tokenize(&self, text: &str) -> Result<serde_json::Value, AppError> {
        let url = format!("{}/v1/tokenize", self.base_url);
        let resp = self
            .http_client
            .post(&url)
            .header("api-key", &self.api_key)
            .json(&json!({
                "text": text,
                "model": crate::constants::DEFAULT_MODEL
            }))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("tokenize request failed: {e}")))?;

        if resp.status().as_u16() == 429 {
            return Err(AppError::RateLimited);
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(AppError::Internal(format!(
                "tokenize returned {status}: {body}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| AppError::Internal(format!("tokenize parse failed: {e}")))
    }

    /// Synthesize text to speech audio via MIMO chat completions API.
    ///
    /// Uses `POST /v1/chat/completions` with `audio` modality.
    /// Returns raw WAV bytes on success.
    pub async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        model: &str,
        _speed: f64,
    ) -> Result<Vec<u8>, AppError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: "user".to_string(),
                    content: String::new(),
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: text.to_string(),
                },
            ],
            audio: AudioParams {
                format: "wav".to_string(),
                voice: voice.to_string(),
            },
            stream: false,
        };

        let resp = self
            .http_client
            .post(&url)
            .header("api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("synthesize request failed: {e}")))?;

        if resp.status().as_u16() == 429 {
            return Err(AppError::RateLimited);
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(AppError::Internal(format!(
                "synthesize returned {status}: {body}"
            )));
        }

        let completion: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("synthesize parse response failed: {e}")))?;

        let choice = completion
            .choices
            .first()
            .ok_or_else(|| AppError::Internal("synthesize returned no choices".to_string()))?;

        let audio = choice
            .message
            .audio
            .as_ref()
            .ok_or_else(|| AppError::Internal("synthesize returned no audio data".to_string()))?;

        base64::engine::general_purpose::STANDARD
            .decode(&audio.data)
            .map_err(|e| AppError::Internal(format!("synthesize base64 decode failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    const WAV_BYTES: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, // RIFF
        0x24, 0x00, 0x00, 0x00, // file size
        0x57, 0x41, 0x56, 0x45, // WAVE
        0x66, 0x6d, 0x74, 0x20, // fmt
        0x10, 0x00, 0x00, 0x00, // chunk size
        0x01, 0x00,             // PCM
        0x01, 0x00,             // mono
        0x44, 0xac, 0x00, 0x00, // 44100 Hz
        0x88, 0x58, 0x01, 0x00, // byte rate
        0x02, 0x00,             // block align
        0x10, 0x00,             // 16-bit
        0x64, 0x61, 0x74, 0x61, // data
        0x00, 0x00, 0x00, 0x00, // data size
    ];

    #[actix_rt::test]
    async fn test_synthesize_returns_wav_bytes() {
        let mock_server = MockServer::start().await;

        let encoded = base64::engine::general_purpose::STANDARD.encode(WAV_BYTES);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("api-key", "test-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "audio": {
                            "data": encoded
                        }
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = MimoClient::new("test-key-123", &mock_server.uri());
        let result = client.synthesize("你好世界", "test-voice", crate::constants::DEFAULT_MODEL, 1.0).await.unwrap();

        assert!(!result.is_empty(), "Should return WAV bytes");
        assert_eq!(result.len(), WAV_BYTES.len());
    }

    #[actix_rt::test]
    async fn test_synthesize_rate_limited() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let client = MimoClient::new("test-key-123", &mock_server.uri());
        let result = client.synthesize("hello", "v", "m", 1.0).await;

        match result {
            Err(AppError::RateLimited) => {} // expected
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[actix_rt::test]
    async fn test_synthesize_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal error"))
            .mount(&mock_server)
            .await;

        let client = MimoClient::new("test-key-123", &mock_server.uri());
        let result = client.synthesize("hello", "v", "m", 1.0).await;

        match result {
            Err(AppError::Internal(msg)) => {
                assert!(msg.contains("500"), "Error should contain status code");
            }
            _ => panic!("Expected Internal error"),
        }
    }

    #[actix_rt::test]
    async fn test_synthesize_timeout() {
        // Simulate timeout by having the mock server delay
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_millis(500)))
            .mount(&mock_server)
            .await;

        // Create client with a very short timeout
        let client = MimoClient {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(50))
                .build()
                .unwrap(),
            api_key: "test-key-123".to_string(),
            base_url: mock_server.uri(),
        };

        let result = client.synthesize("hello", "v", "m", 1.0).await;
        match result {
            Err(AppError::Internal(msg)) => {
                assert!(
                    msg.contains("timeout") || msg.contains("timed out") || msg.contains("failed"),
                    "Error should mention timeout or failure: {msg}"
                );
            }
            other => panic!("Expected Internal error due to timeout, got: {other:?}"),
        }
    }

    #[actix_rt::test]
    async fn test_tokenize_authorization() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/tokenize"))
            .and(header("Authorization", "Bearer test-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sentences": [{"text": "hello", "token_count": 1, "char_count": 5}]
            })))
            .mount(&mock_server)
            .await;

        let client = MimoClient::new("test-key-123", &mock_server.uri());
        let result = client.tokenize("hello").await.unwrap();
        assert_eq!(result["sentences"][0]["text"], "hello");
    }
}
