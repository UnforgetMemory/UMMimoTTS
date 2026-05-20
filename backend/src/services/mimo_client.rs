use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing;

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
                .timeout(std::time::Duration::from_secs(120))
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

        let mut messages = Vec::new();

        // 如果有 context，添加到消息列表
        if let Some(ctx) = context {
            if !ctx.trim().is_empty() {
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: ctx.to_string(),
                });
            }
        }

        // 添加要合成的文本
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

        tracing::info!(
            "Sending TTS request to MIMO API: model={}, voice={}",
            model,
            voice
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("MIMO API error: {} - {}", status, error_text);

            return match status.as_u16() {
                401 => Err(MimoError::InvalidApiKey),
                429 => Err(MimoError::RateLimitExceeded),
                _ => Err(MimoError::ApiError {
                    code: status.as_u16().to_string(),
                    message: error_text,
                }),
            };
        }

        let completion: ChatCompletionResponse = response.json().await?;

        // 提取音频数据
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
}
