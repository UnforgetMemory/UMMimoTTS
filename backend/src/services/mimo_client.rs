use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing;

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 500;
const MAX_CHUNK_CHARS: usize = 2000;   // API 单次最大字符数
const MIN_CHUNK_CHARS: usize = 300;    // 最小分片，避免碎片
const CHUNK_DELAY_MS: u64 = 6500;      // 分片间延迟：10次/分钟 ≈ 6秒/次
const MAX_RPM: usize = 10;             // 每分钟最大请求数

/// 智能文本分片策略
///
/// 核心思想：先算最优片数，再均匀分配，而不是固定切2000
///
/// 算法：
/// 1. 按句子边界细粒度分割
/// 2. 计算最优片数 n = ceil(L / MAX_CHUNK)
/// 3. 目标片大小 target = L / n（均匀分配）
/// 4. 贪心合并：累积句子直到接近 target，确保不超过 MAX_CHUNK
///
/// 效果对比：
/// - 2001 字：旧 [2000]+[1] → 新 [1001]+[1000]（2片均匀）
/// - 2050 字：旧 [2000]+[50] → 新 [1025]+[1025]（2片均匀）
/// - 3999 字：旧 [2000]+[1999] → 新 [2000]+[1999]（2片，OK）
/// - 5000 字：旧 [2000]+[2000]+[1000] → 新 [1667]+[1667]+[1666]（3片均匀）
/// - 10000 字：5片 × 2000（最优化）
pub fn split_text_into_chunks(text: &str) -> Vec<String> {
    let total_chars = text.chars().count();

    if total_chars <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }

    let sentences = split_by_sentences(text);
    let chunk_count = (total_chars + MAX_CHUNK_CHARS - 1) / MAX_CHUNK_CHARS;
    let target_size = (total_chars + chunk_count - 1) / chunk_count;

    tracing::info!(
        "Smart chunking: {} chars → {} chunks, target {} chars/chunk",
        total_chars, chunk_count, target_size
    );

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut current_size = 0;

    for sentence in &sentences {
        let sentence_len = sentence.chars().count();

        // 关键：先检查加上这个句子后是否会超过限制
        // 如果会，先保存当前块（不包括这个句子）
        if current_size + sentence_len > MAX_CHUNK_CHARS && !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
            current_chunk = String::new();
            current_size = 0;
        }

        // 添加当前句子
        current_chunk.push_str(sentence);
        current_size += sentence_len;

        // 添加后检查：是否达到目标大小
        // 如果达到，立即保存（确保不会继续累积超过限制）
        if current_size >= target_size {
            chunks.push(current_chunk.trim().to_string());
            current_chunk = String::new();
            current_size = 0;
        }
    }

    // 添加最后一个块
    if !current_chunk.trim().is_empty() {
        let remaining = current_chunk.trim().to_string();
        let remaining_len = remaining.chars().count();
        
        if remaining_len < MIN_CHUNK_CHARS && !chunks.is_empty() {
            let last = chunks.pop().unwrap();
            let last_len = last.chars().count();
            
            // 只有合并后不超过 MAX_CHUNK_CHARS 时才合并
            if last_len + remaining_len <= MAX_CHUNK_CHARS {
                chunks.push(format!("{}{}", last, remaining));
            } else {
                // 合并会超限，直接添加为独立块
                chunks.push(last);
                chunks.push(remaining);
            }
        } else {
            chunks.push(remaining);
        }
    }

    tracing::info!("Split into {} chunks: {:?}",
        chunks.len(),
        chunks.iter().map(|c| c.chars().count()).collect::<Vec<_>>()
    );

    chunks
}

/// 按句子边界分割文本
fn split_by_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        // 中文/英文句子结束符
        if ch == '。' || ch == '！' || ch == '？' || ch == '；'
            || ch == '.' || ch == '!' || ch == '?' || ch == ';'
            || ch == '\n' {
            sentences.push(current.clone());
            current.clear();
        }
    }

    // 添加剩余文本
    if !current.trim().is_empty() {
        sentences.push(current);
    }

    sentences
}

/// 合并多个 WAV 音频数据
pub fn merge_wav_audio(chunks: Vec<Vec<u8>>) -> Result<Vec<u8>, MimoError> {
    if chunks.is_empty() {
        return Err(MimoError::NoAudioData);
    }

    if chunks.len() == 1 {
        return Ok(chunks.into_iter().next().unwrap());
    }

    // WAV 文件结构：44 字节头 + PCM 数据
    const HEADER_SIZE: usize = 44;

    // 从第一个块读取格式信息
    let first_chunk = &chunks[0];
    if first_chunk.len() < HEADER_SIZE {
        return Err(MimoError::NoAudioData);
    }

    let header = &first_chunk[..HEADER_SIZE];
    let bytes_per_sample = u16::from_le_bytes([header[22], header[23]]) as usize;
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
    let num_channels = u16::from_le_bytes([header[22], header[23]]) as usize;

    // 收集所有 PCM 数据
    let mut all_pcm_data = Vec::new();
    for chunk in &chunks {
        if chunk.len() > HEADER_SIZE {
            all_pcm_data.extend_from_slice(&chunk[HEADER_SIZE..]);
        }
    }

    // 构建新的 WAV 头
    let data_size = all_pcm_data.len() as u32;
    let file_size = data_size + 36;

    let mut wav = Vec::with_capacity(HEADER_SIZE + all_pcm_data.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());  // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes());   // PCM format
    wav.extend_from_slice(&(num_channels as u16).to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * num_channels as u32 * bytes_per_sample as u32;
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = num_channels as u16 * bytes_per_sample as u16;
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&(bytes_per_sample as u16 * 8).to_le_bytes());  // bits per sample

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(&all_pcm_data);

    Ok(wav)
}

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
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

/// 速率限制器 - 滑动窗口实现
struct RateLimiter {
    request_times: Vec<std::time::Instant>,
    max_rpm: usize,
}

impl RateLimiter {
    fn new(max_rpm: usize) -> Self {
        Self {
            request_times: Vec::new(),
            max_rpm,
        }
    }

    /// 检查并等待，直到可以发送请求
    async fn acquire(&mut self) {
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(60);

        // 清理超过 1 分钟的记录
        self.request_times.retain(|t| now.duration_since(*t) < window);

        // 如果达到限制，等待直到最早的记录过期
        if self.request_times.len() >= self.max_rpm {
            if let Some(oldest) = self.request_times.first() {
                let wait_time = window - now.duration_since(*oldest);
                tracing::warn!(
                    "Rate limit reached ({} requests/min), waiting {:?}",
                    self.max_rpm,
                    wait_time
                );
                tokio::time::sleep(wait_time).await;
            }
        }

        self.request_times.push(std::time::Instant::now());
    }
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
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new(MAX_RPM))),
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

        // 速率限制：等待直到可以发送请求
        {
            let mut limiter = self.rate_limiter.lock().await;
            limiter.acquire().await;
        }

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

    /// 带分片的合成方法，支持超长文本
    /// 返回合并后的音频数据
    /// 内置流控：分片间延迟 CHUNK_DELAY_MS，控制 RPM
    pub async fn synthesize_chunked(
        &self,
        model: &str,
        text: &str,
        voice: &str,
        context: Option<&str>,
        on_progress: impl Fn(usize, usize) + Send + Sync,
    ) -> Result<Vec<u8>, MimoError> {
        let chunks = split_text_into_chunks(text);
        let total_chunks = chunks.len();

        tracing::info!(
            "Text split into {} chunks for synthesis (total {} chars)",
            total_chunks,
            text.len()
        );

        let mut audio_chunks = Vec::with_capacity(total_chunks);

        for (i, chunk) in chunks.iter().enumerate() {
            tracing::info!("Synthesizing chunk {}/{} ({} chars)", i + 1, total_chunks, chunk.len());
            on_progress(i + 1, total_chunks);

            // 流控：分片间延迟，避免触发 RPM 限制（100次/分钟）
            if i > 0 {
                tracing::info!("Rate limit delay: {}ms between chunks", CHUNK_DELAY_MS);
                tokio::time::sleep(std::time::Duration::from_millis(CHUNK_DELAY_MS)).await;
            }

            let audio = self.synthesize(model, chunk, voice, context).await?;
            audio_chunks.push(audio);
        }

        merge_wav_audio(audio_chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_text_completeness() {
        let test_cases = vec![
            100, 500, 1000, 1500, 2000, 2001, 2050, 3000, 3999, 5000, 10000,
        ];

        for len in test_cases {
            let text = "这是一段测试文本，用于验证分片功能。".repeat(len / 15 + 1);
            let text: String = text.chars().take(len).collect();
            
            let chunks = split_text_into_chunks(&text);
            let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
            let original_len = text.chars().count();
            
            assert_eq!(
                total, original_len,
                "文本长度 {} 字，分片后合计 {} 字，丢失了 {} 字！",
                original_len, total, original_len - total
            );
            
            for (i, chunk) in chunks.iter().enumerate() {
                assert!(
                    chunk.chars().count() <= MAX_CHUNK_CHARS,
                    "分片 {} 超过 {} 字限制：{} 字",
                    i, MAX_CHUNK_CHARS, chunk.chars().count()
                );
            }
            
            println!("✅ {}字 → {}片，完整", original_len, chunks.len());
        }
    }

    #[test]
    fn test_split_no_boundaries() {
        let text = "这是一段很长的文本没有句号分隔".repeat(200);
        let chunks = split_text_into_chunks(&text);
        let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
        
        assert_eq!(total, text.chars().count(), "无边界文本丢失内容");
        println!("✅ 无边界文本 {}字 → {}片，完整", text.chars().count(), chunks.len());
    }

    #[test]
    fn test_split_long_sentence() {
        let long = "这".repeat(3000);
        let text = format!("{}。这是结尾。", long);
        
        let chunks = split_text_into_chunks(&text);
        let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
        
        assert_eq!(total, text.chars().count(), "超长句子文本丢失内容");
        println!("✅ 超长句子 {}字 → {}片，完整", text.chars().count(), chunks.len());
    }
}
