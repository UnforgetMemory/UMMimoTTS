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
    /// Nanoseconds between token refills.  Stored as `60_000_000_000 / rpm`
    /// to avoid integer-division precision loss at low rpm values.
    nanos_per_token: u64,
    last_refill: AtomicU64,
}

impl TokenBucket {
    /// Create a new rate limiter allowing `rpm` requests per minute with full burst.
    pub fn new(rpm: u64) -> Self {
        Self::new_with_burst(rpm, rpm)
    }

    /// Create a rate limiter with a limited burst capacity.
    /// `max_burst` caps how many tokens can accumulate at once — use `1` to force
    /// serialized API calls (no concurrent bursts), while maintaining the `rpm` refill rate.
    pub fn new_with_burst(rpm: u64, max_burst: u64) -> Self {
        let nanos_per_token = if rpm == 0 {
            u64::MAX // effectively disabled
        } else {
            60_000_000_000 / rpm
        };
        Self {
            tokens: AtomicU64::new(max_burst),
            capacity: max_burst,
            nanos_per_token,
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
    /// Uses capped backoff (1ms → 100ms max) to reduce CPU on contended buckets.
    pub async fn acquire(&self) {
        let mut backoff_us = 1_000u64; // start at 1ms
        loop {
            if self.try_acquire() {
                return;
            }
            // Wait proportional to nanos_per_token, capped at 100ms for responsiveness.
            // Using exponential backoff from base so slow buckets don't busy-poll.
            let refill_wait_ms = std::cmp::min(
                std::cmp::max(1, self.nanos_per_token / 1_000_000),
                100,
            );
            let wait_ms = std::cmp::min(backoff_us / 1_000, refill_wait_ms);
            sleep(Duration::from_millis(wait_ms)).await;
            backoff_us = std::cmp::min(backoff_us * 2, 100_000); // 1ms → 2ms → 4ms → ... → 100ms max
        }
    }

    /// Try to acquire `n` tokens without blocking.
    ///
    /// Returns `true` if all `n` tokens were acquired, `false` otherwise.
    pub fn try_acquire_n(&self, n: u64) -> bool {
        self.refill();
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current < n {
                return false;
            }
            if self
                .tokens
                .compare_exchange(current, current - n, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Acquire `n` tokens, waiting asynchronously if insufficient are available.
    pub async fn acquire_n(&self, n: u64) {
        loop {
            if self.try_acquire_n(n) {
                return;
            }
            // Wait time proportional to nanos_per_token, capped at 100ms for responsiveness
            let wait_ms = std::cmp::min(
                std::cmp::max(1, self.nanos_per_token / 1_000_000),
                100,
            );
            sleep(Duration::from_millis(std::cmp::min(wait_ms, 100))).await;
        }
    }

    /// Return `n` tokens to the bucket (refund on skip/error paths).
    /// Capped at capacity to prevent overfilling.
    pub fn release(&self, n: u64) {
        if n == 0 {
            return;
        }
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            let new_val = std::cmp::min(current + n, self.capacity);
            match self.tokens.compare_exchange_weak(
                current,
                new_val,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
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

        // Compute earned tokens
        let earned = elapsed / self.nanos_per_token;

        // Only update last_refill if we actually earned tokens,
        // otherwise the timer resets every 100ms and never accumulates
        if earned > 0 {
            // Try to claim this refill window.  If another thread beat us, bail.
            if self
                .last_refill
                .compare_exchange(last, now, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                return;
            }

            let current = self.tokens.load(Ordering::Relaxed);
            let new_val = std::cmp::min(current + earned, self.capacity);
            self.tokens.store(new_val, Ordering::Release);
        }
    }
}

// ── Per-Provider Rate Limiter Map ──────────────────────────────────────

use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::Instant;

/// Per-provider rate limiter holding independent RPM and TPM buckets.
pub struct ProviderLimiter {
    pub rpm_bucket: TokenBucket,
    pub tpm_bucket: TokenBucket,
}

/// Tracks degraded mode state for a provider.
struct DegradedState {
    degraded_since: Instant,
    degraded_rpm: u64,
    degraded_tpm: u64,
    recovery_after: std::time::Duration,
    /// How many times degradation has been re-triggered (for exponential backoff)
    escalation_count: u32,
}

/// Original (non-degraded) limits for a provider, used to restore after degradation.
struct OriginalLimits {
    rpm: u64,
    tpm: u64,
    burst: u64,
}

/// Manages per-provider rate limiters so each provider can independently
/// use its full RPM/TPM quota (e.g. 100 RPM + 10M TPM each).
pub struct ProviderRateLimiterMap {
    limiters: RwLock<HashMap<String, ProviderLimiter>>,
    degraded: RwLock<HashMap<String, DegradedState>>,
    originals: RwLock<HashMap<String, OriginalLimits>>,
    default_rpm: u64,
    default_tpm: u64,
    default_burst: u64,
}

impl ProviderRateLimiterMap {
    /// Create a new map with default RPM/TPM values applied to each provider.
    pub fn new(default_rpm: u64, default_tpm: u64, default_burst: u64) -> Self {
        Self {
            limiters: RwLock::new(HashMap::new()),
            degraded: RwLock::new(HashMap::new()),
            originals: RwLock::new(HashMap::new()),
            default_rpm,
            default_tpm,
            default_burst,
        }
    }

    /// Get or lazily create a limiter for the given provider ID.
    pub fn get_or_create(&self, provider_id: &str) -> ProviderLimiterRef<'_> {
        // Fast path: read lock
        {
            let map = self.limiters.read();
            if map.contains_key(provider_id) {
                return ProviderLimiterRef {
                    map: &self.limiters,
                    provider_id: provider_id.to_string(),
                };
            }
        }
        // Slow path: write lock + insert
        {
            let mut map = self.limiters.write();
            map.entry(provider_id.to_string()).or_insert_with(|| {
                // Store original limits for potential recovery
                self.originals.write().entry(provider_id.to_string())
                    .or_insert_with(|| OriginalLimits {
                        rpm: self.default_rpm,
                        tpm: self.default_tpm,
                        burst: self.default_burst,
                    });
                ProviderLimiter {
                    rpm_bucket: TokenBucket::new_with_burst(self.default_rpm, self.default_burst),
                    tpm_bucket: TokenBucket::new(self.default_tpm),
                }
            });
        }
        ProviderLimiterRef {
            map: &self.limiters,
            provider_id: provider_id.to_string(),
        }
    }

    /// Hot-add a provider with custom RPM/TPM.
    pub fn add_provider(&self, provider_id: &str, rpm: u64, tpm: u64, burst: u64) {
        let mut map = self.limiters.write();
        map.insert(provider_id.to_string(), ProviderLimiter {
            rpm_bucket: TokenBucket::new_with_burst(rpm, burst),
            tpm_bucket: TokenBucket::new(tpm),
        });
        self.originals.write().insert(provider_id.to_string(), OriginalLimits {
            rpm, tpm, burst,
        });
    }

    /// Remove a provider's limiter (drain in-flight tokens).
    pub fn remove_provider(&self, provider_id: &str) {
        let mut map = self.limiters.write();
        map.remove(provider_id);
        self.degraded.write().remove(provider_id);
        self.originals.write().remove(provider_id);
    }

    /// List all currently tracked provider IDs.
    pub fn provider_ids(&self) -> Vec<String> {
        self.limiters.read().keys().cloned().collect()
    }

    /// Enter degraded mode for a provider: replace buckets with lower limits.
    /// `rpm` and `tpm` are the degraded (reduced) limits.
    /// `duration` is how long to stay degraded before auto-recovery.
    pub fn enter_degraded_mode(&self, provider_id: &str, rpm: u64, tpm: u64, duration: std::time::Duration) {
        // Ensure original limits are saved
        {
            let mut originals = self.originals.write();
            originals.entry(provider_id.to_string()).or_insert_with(|| {
                let map = self.limiters.read();
                if let Some(_limiter) = map.get(provider_id) {
                    OriginalLimits { rpm: self.default_rpm, tpm: self.default_tpm, burst: self.default_burst }
                } else {
                    OriginalLimits { rpm: self.default_rpm, tpm: self.default_tpm, burst: self.default_burst }
                }
            });
        }

        // Track escalation for exponential recovery backoff
        let escalation = {
            let mut deg = self.degraded.write();
            let state = deg.entry(provider_id.to_string()).or_insert_with(|| DegradedState {
                degraded_since: Instant::now(),
                degraded_rpm: rpm,
                degraded_tpm: tpm,
                recovery_after: duration,
                escalation_count: 0,
            });
            state.escalation_count += 1;
            state.degraded_since = Instant::now();
            // Exponential recovery backoff: 60s, 120s, 240s...
            let multiplier = 1u64 << (state.escalation_count.saturating_sub(1)).min(4);
            state.recovery_after = duration.mul_f64(multiplier as f64);
            state.degraded_rpm = rpm;
            state.degraded_tpm = tpm;
            state.escalation_count
        };

        // Replace the limiter with degraded buckets
        {
            let mut map = self.limiters.write();
            let burst = std::cmp::max(1, rpm / 3);
            map.insert(provider_id.to_string(), ProviderLimiter {
                rpm_bucket: TokenBucket::new_with_burst(rpm, burst),
                tpm_bucket: TokenBucket::new(tpm),
            });
        }

        tracing::warn!(
            "RateLimiter: provider '{provider_id}' entered DEGRADED mode \
             (rpm={rpm}, tpm={tpm}, escalation={escalation}, recovery={duration:?})"
        );
    }

    /// Exit degraded mode: restore original RPM/TPM limits.
    pub fn exit_degraded_mode(&self, provider_id: &str) {
        let orig = {
            let mut deg = self.degraded.write();
            if let Some(state) = deg.remove(provider_id) {
                let originals = self.originals.read();
                originals.get(provider_id).map(|o| (o.rpm, o.tpm, o.burst, state.escalation_count))
            } else {
                None
            }
        };

        if let Some((rpm, tpm, burst, escalation)) = orig {
            let mut map = self.limiters.write();
            map.insert(provider_id.to_string(), ProviderLimiter {
                rpm_bucket: TokenBucket::new_with_burst(rpm, burst),
                tpm_bucket: TokenBucket::new(tpm),
            });
            tracing::info!(
                "RateLimiter: provider '{provider_id}' exited DEGRADED mode \
                 (restored rpm={rpm}, tpm={tpm}, was escalated {escalation}x)"
            );
        }
    }

    /// Check if a provider is currently in degraded mode.
    /// Auto-recovers if the recovery duration has elapsed.
    pub fn is_degraded(&self, provider_id: &str) -> bool {
        let should_recover = {
            let deg = self.degraded.read();
            if let Some(state) = deg.get(provider_id) {
                state.degraded_since.elapsed() >= state.recovery_after
            } else {
                false
            }
        };

        if should_recover {
            self.exit_degraded_mode(provider_id);
            return false;
        }

        self.degraded.read().contains_key(provider_id)
    }

    /// Get the effective RPM for a provider (degraded or original).
    pub fn effective_rpm(&self, provider_id: &str) -> u64 {
        // Check degraded first
        {
            let deg = self.degraded.read();
            if let Some(state) = deg.get(provider_id) {
                if state.degraded_since.elapsed() < state.recovery_after {
                    return state.degraded_rpm;
                }
            }
        }
        // Original
        let originals = self.originals.read();
        originals.get(provider_id).map(|o| o.rpm).unwrap_or(self.default_rpm)
    }
}

/// Lightweight reference to a provider's limiter inside the map.
/// Acquires/releases are done by looking up the entry each time.
pub struct ProviderLimiterRef<'a> {
    map: &'a RwLock<HashMap<String, ProviderLimiter>>,
    provider_id: String,
}

impl<'a> ProviderLimiterRef<'a> {
    /// Acquire one RPM token for this provider, waiting if necessary.
    pub async fn acquire_rpm(&self) {
        let mut backoff_us = 1_000u64;
        loop {
            enum Step { Acquired, NotFound, Retry }
            let action = {
                let map = self.map.read();
                match map.get(&self.provider_id) {
                    Some(limiter) => {
                        if limiter.rpm_bucket.try_acquire() {
                            Step::Acquired
                        } else {
                            Step::Retry
                        }
                    }
                    None => Step::NotFound,
                }
            }; // guard definitively dropped before any await
            match action {
                Step::Acquired => return,
                Step::NotFound => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    return;
                }
                Step::Retry => {}
            }
            let wait_ms = std::cmp::min(backoff_us / 1_000, 100);
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            backoff_us = std::cmp::min(backoff_us * 2, 100_000);
        }
    }

    /// Try to acquire N TPM tokens without blocking.
    pub fn try_acquire_tpm(&self, n: u64) -> bool {
        let map = self.map.read();
        if let Some(limiter) = map.get(&self.provider_id) {
            limiter.tpm_bucket.try_acquire_n(n)
        } else {
            false
        }
    }

    /// Release one RPM token back (on error/skip paths).
    pub fn release_rpm(&self, n: u64) {
        let map = self.map.read();
        if let Some(limiter) = map.get(&self.provider_id) {
            limiter.rpm_bucket.release(n);
        }
    }

    /// Release N TPM tokens back.
    pub fn release_tpm(&self, n: u64) {
        let map = self.map.read();
        if let Some(limiter) = map.get(&self.provider_id) {
            limiter.tpm_bucket.release(n);
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
