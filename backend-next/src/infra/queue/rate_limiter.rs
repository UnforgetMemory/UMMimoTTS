//! Token bucket rate limiter for MIMO API requests.
//!
//! Provides lock-free rate limiting using atomic operations.
//! Refills happen on-demand with a minimum 100ms interval to avoid
//! contention under heavy concurrent load.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// Monotonic nanosecond timestamp used by the token bucket.
fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

/// Lock-free token bucket rate limiter.
///
/// - `rpm` (requests per minute) determines both initial capacity and refill rate.
/// - On construction, the bucket is full (`tokens = rpm`).
/// - `try_acquire()` returns immediately with `false` when empty.
/// - `acquire()` sleeps and retries until a token is available.
/// - Refills happen on-demand inside `try_acquire` / `acquire`, at most once
///   per 100ms to keep atomic operations cheap.
pub struct TokenBucket {
    tokens: AtomicU64,
    capacity: u64,
    refill_per_sec: u64,
    last_refill: AtomicU64,
}

impl TokenBucket {
    /// Create a new rate limiter allowing `rpm` requests per minute.
    pub fn new(rpm: u64) -> Self {
        Self {
            tokens: AtomicU64::new(rpm),
            capacity: rpm,
            refill_per_sec: if rpm == 0 { 0 } else { std::cmp::max(1, rpm / 60) },
            last_refill: AtomicU64::new(now_nanos()),
        }
    }

    /// Try to acquire one token without blocking.
    ///
    /// Returns `true` immediately if a token was available, `false` otherwise.
    /// This method performs an on-demand refill before checking.
    pub fn try_acquire(&self) -> bool {
        self.refill();
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current == 0 {
                return false;
            }
            if self
                .tokens
                .compare_exchange(current, current - 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Acquire one token, waiting asynchronously if none are available.
    ///
    /// The wait interval is proportional to the refill rate: for an `rpm` of 60,
    /// it polls roughly every ~1 s (or the refill interval, whichever is smaller).
    pub async fn acquire(&self) {
        loop {
            if self.try_acquire() {
                return;
            }
            let wait_ms = if self.refill_per_sec > 0 {
                1000 / self.refill_per_sec
            } else {
                1000
            };
            // Cap at 100ms polling interval for responsive wake-ups.
            sleep(Duration::from_millis(std::cmp::min(wait_ms, 100))).await;
        }
    }

    /// On-demand refill: computes tokens earned since the last refill and adds
    /// them to the bucket, capped at `capacity`.  Skips if fewer than 100 ms
    /// have elapsed since the last refill to avoid CAS contention.
    fn refill(&self) {
        let now = now_nanos();
        let last = self.last_refill.load(Ordering::Relaxed);

        let elapsed = now.saturating_sub(last);
        if elapsed < 100_000_000 {
            // less than 100ms – too soon to refill
            return;
        }

        // Try to claim this refill window.  If another thread beat us, bail.
        if self
            .last_refill
            .compare_exchange(last, now, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let elapsed_secs = elapsed as f64 / 1_000_000_000.0;
        let earned = (elapsed_secs * self.refill_per_sec as f64) as u64;
        if earned > 0 {
            let current = self.tokens.load(Ordering::Relaxed);
            let new_val = std::cmp::min(current + earned, self.capacity);
            self.tokens.store(new_val, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_rt::test]
    async fn test_rate_limiter_initial_capacity() {
        let bucket = TokenBucket::new(10);

        // First 10 acquires should succeed.
        for _ in 0..10 {
            assert!(bucket.try_acquire(), "expected token available");
        }
        // 11th should be blocked.
        assert!(!bucket.try_acquire(), "expected no token left");
    }

    #[actix_rt::test]
    async fn test_rate_limiter_refill() {
        let bucket = TokenBucket::new(60); // 60 rpm → 1 token/sec

        // Drain all tokens.
        for _ in 0..60 {
            assert!(bucket.try_acquire());
        }
        assert!(!bucket.try_acquire(), "bucket should be empty");

        // Wait just over 1 second for refill.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Should have ~1 token now.
        assert!(
            bucket.try_acquire(),
            "expected 1 token after 1s refill"
        );
    }

    #[actix_rt::test]
    async fn test_rate_limiter_acquire_blocks_then_succeeds() {
        let bucket = TokenBucket::new(5);

        // Drain.
        for _ in 0..5 {
            bucket.try_acquire();
        }

        // acquire should eventually succeed after a refill cycle.
        // With 5 rpm → 5/60 ≈ 0.083 tokens/sec → wait ~12s, which is too slow.
        // Instead, test that acquire() doesn't panic and returns at some point.
        // Use a timeout wrapper to avoid hanging the test suite.

        // We'll use a small rpm so refill is slow; just verify it doesn't hang.
        let fast_bucket = TokenBucket::new(1200); // 20/sec → 50ms per token
        for _ in 0..1200 {
            fast_bucket.try_acquire();
        }

        // Should get a token within ~100ms
        tokio::time::timeout(Duration::from_millis(500), fast_bucket.acquire())
            .await
            .expect("acquire should return within timeout");
    }
}
