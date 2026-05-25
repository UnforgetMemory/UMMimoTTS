use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Global rate limiter for MimoAPI
/// Enforces: 90 RPM (requests per minute) + 5M TPM (tokens per minute)
#[derive(Clone)]
pub struct GlobalRateLimiter {
    inner: Arc<Mutex<RateLimiterInner>>,
}

struct RateLimiterInner {
    // Request rate limiting (sliding window)
    request_timestamps: VecDeque<Instant>,
    max_rpm: u64,

    // Token rate limiting (sliding window)
    token_timestamps: VecDeque<(Instant, usize)>, // (timestamp, token_count)
    max_tpm: usize,

    // Statistics
    total_requests: u64,
    total_tokens: usize,
    total_wait_time_ms: u64,
}

impl GlobalRateLimiter {
    pub fn new(max_rpm: u64, max_tpm: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RateLimiterInner {
                request_timestamps: VecDeque::new(),
                max_rpm,
                token_timestamps: VecDeque::new(),
                max_tpm,
                total_requests: 0,
                total_tokens: 0,
                total_wait_time_ms: 0,
            })),
        }
    }

    /// Wait until a request slot is available
    pub async fn acquire_request_slot(&self) {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(60);

        // Clean old timestamps
        while let Some(&front) = inner.request_timestamps.front() {
            if now.duration_since(front) > window {
                inner.request_timestamps.pop_front();
            } else {
                break;
            }
        }

        // If at limit, calculate wait time
        if inner.request_timestamps.len() >= inner.max_rpm as usize {
            if let Some(&oldest) = inner.request_timestamps.front() {
                let wait_time = window - now.duration_since(oldest);
                if !wait_time.is_zero() {
                    inner.total_wait_time_ms += wait_time.as_millis() as u64;
                    drop(inner); // Release lock before sleeping
                    tokio::time::sleep(wait_time).await;
                    // Re-acquire and record
                    let mut inner = self.inner.lock().await;
                    inner.request_timestamps.push_back(Instant::now());
                    inner.total_requests += 1;
                    return;
                }
            }
        }

        // Under limit, record immediately
        inner.request_timestamps.push_back(now);
        inner.total_requests += 1;
    }

    /// Wait until token budget is available
    pub async fn acquire_token_budget(&self, token_count: usize) {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(60);

        // Clean old entries
        while let Some(&(front_ts, _)) = inner.token_timestamps.front() {
            if now.duration_since(front_ts) > window {
                inner.token_timestamps.pop_front();
            } else {
                break;
            }
        }

        // Calculate current token usage in window
        let current_tokens: usize = inner.token_timestamps.iter().map(|(_, t)| t).sum();

        if current_tokens + token_count > inner.max_tpm {
            // Need to wait for oldest entries to expire
            if let Some(&(oldest_ts, _)) = inner.token_timestamps.front() {
                let wait_time = window - now.duration_since(oldest_ts);
                if !wait_time.is_zero() {
                    inner.total_wait_time_ms += wait_time.as_millis() as u64;
                    drop(inner);
                    tokio::time::sleep(wait_time).await;
                    let mut inner = self.inner.lock().await;
                    inner.token_timestamps.push_back((Instant::now(), token_count));
                    inner.total_tokens += token_count;
                    return;
                }
            }
        }

        inner.token_timestamps.push_back((now, token_count));
        inner.total_tokens += token_count;
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> RateLimiterStats {
        let inner = self.inner.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(60);

        // Current RPM
        let current_rpm = inner
            .request_timestamps
            .iter()
            .filter(|ts| now.duration_since(**ts) <= window)
            .count();

        // Current TPM
        let current_tpm: usize = inner
            .token_timestamps
            .iter()
            .filter(|(ts, _)| now.duration_since(*ts) <= window)
            .map(|(_, tokens)| tokens)
            .sum();

        RateLimiterStats {
            current_rpm,
            max_rpm: inner.max_rpm,
            current_tpm,
            max_tpm: inner.max_tpm,
            total_requests: inner.total_requests,
            total_tokens: inner.total_tokens,
            total_wait_time_ms: inner.total_wait_time_ms,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RateLimiterStats {
    pub current_rpm: usize,
    pub max_rpm: u64,
    pub current_tpm: usize,
    pub max_tpm: usize,
    pub total_requests: u64,
    pub total_tokens: usize,
    pub total_wait_time_ms: u64,
}
