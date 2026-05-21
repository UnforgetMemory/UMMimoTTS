use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing;

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 500;

#[derive(Error, Debug)]
pub enum MimoError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("API error: {code} - {message}")]
    ApiError { code: String, message: String },

    #[error("No audio data in response")]
    NoAudioData,

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("All retries exhausted: {0}")]
    RetryExhausted(String),
}

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
}

#[derive(Debug, Deserialize)]
struct AudioData {
    data: String,
}

#[derive(Debug, Deserialize)]
struct Message {
    audio: Option<AudioData>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

pub struct MimoClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl MimoClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(300))  // 5 分钟超时，大文本需要更长时间
                .connect_timeout(std::time::Duration::from_secs(30))  // 连接超时 30 秒
                .pool_max_idle_per_host(10)  // 连接池：每主机最大空闲连接数
                .pool_idle_timeout(std::time::Duration::from_secs(90))  // 空闲连接存活时间
                .build()
                .expect("Failed to create HTTP client"),
            api_key,
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
        }
    }

    pub async fn synthesize(
        &self,
        model: &str,
        text: &str,
        voice: &str,
        context: Option<&str>,
    ) -> Result<Vec<u8>, MimoError> {
        let url = format!("{}/chat/completions", self.base_url);

        // 构建请求体（只需一次）
        let mut messages = Vec::new();

        if let Some(ctx) = context {
            if !ctx.trim().is_empty() {
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: ctx.to_string(),
                });
            }
        }

        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: text.to_string(),
        });

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            audio: AudioParams {
                format: "wav".to_string(),
                voice: voice.to_string(),
            },
        };

        // 带重试的请求逻辑
        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            tracing::info!(
                "Sending TTS request to MIMO API: model={}, voice={} (attempt {}/{})",
                model,
                voice,
                attempt,
                MAX_RETRIES
            );

            let result = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        return Self::handle_success_response(response).await;
                    }

                    let error_text = response.text().await.unwrap_or_default();
                    tracing::error!("MIMO API error: {} - {}", status, error_text);

                    match status.as_u16() {
                        401 => return Err(MimoError::InvalidApiKey),
                        429 => {
                            last_error = Some(MimoError::RateLimitExceeded);
                            Self::retry_delay(attempt).await;
                            continue;
                        }
                        _ if status.is_server_error() => {
                            last_error = Some(MimoError::ApiError {
                                code: status.as_u16().to_string(),
                                message: error_text.clone(),
                            });
                            Self::retry_delay(attempt).await;
                            continue;
                        }
                        _ => {
                            return Err(MimoError::ApiError {
                                code: status.as_u16().to_string(),
                                message: error_text,
                            });
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Request attempt {}/{} failed: {}",
                        attempt,
                        MAX_RETRIES,
                        e
                    );
                    last_error = Some(MimoError::HttpError(e));
                    if attempt < MAX_RETRIES {
                        Self::retry_delay(attempt).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            MimoError::RetryExhausted("请求失败，请检查网络连接".to_string())
        }))
    }

    async fn handle_success_response(
        response: reqwest::Response,
    ) -> Result<Vec<u8>, MimoError> {
        let completion: ChatCompletionResponse = response.json().await?;

        if let Some(choice) = completion.choices.first() {
            if let Some(audio) = &choice.message.audio {
                let audio_bytes =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &audio.data)
                        .map_err(|e| {
                            tracing::error!("Failed to decode base64 audio: {}", e);
                            MimoError::NoAudioData
                        })?;

                tracing::info!("Audio data decoded: {} bytes", audio_bytes.len());
                Ok(audio_bytes)
            } else {
                Err(MimoError::NoAudioData)
            }
        } else {
            Err(MimoError::NoAudioData)
        }
    }

    async fn retry_delay(attempt: u32) {
        let delay_ms = BASE_RETRY_DELAY_MS * (1 << (attempt - 1));
        let jitter = fastrand::u64(0..delay_ms.min(1000));
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms + jitter)).await;
    }
}
