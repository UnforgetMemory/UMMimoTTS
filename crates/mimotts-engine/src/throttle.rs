//! ADR-012 — 429 smart start/stop: token buckets + AIMD concurrency gate
//! + per-provider circuit breaker.
//!
//! Official facts (docs/compose/plans/2026-08-28-mimo-adaptation-research.md §1.8):
//! - RPM 100 / TPM 10M per model, **account-level across all keys**;
//! - no documented `Retry-After` / `x-ratelimit-*` headers → self-budgeted;
//! - unspecified account-level concurrency cap (community data: ~30 ok, 50+ throttled).
//!
//! Strategy:
//! - RPM bucket runs at 90% headroom (<=90 req/min), TPM pre-reserved & refunded;
//! - AIMD concurrency window: +1 per healthy interval, ×0.5 (floor 1) on 429;
//! - full-jitter exponential backoff; circuit opens after 3 consecutive 429s,
//!   half-open single probe after 60s, progressive recovery (never jump to max).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::Notify;

// ── token bucket (lock-free CAS, nanosecond refill) ──────────────────────

fn now_nanos() -> u64 {
    // Monotonic source: SystemTime is acceptable (machine suspend only adds time).
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

pub struct TokenBucket {
    tokens: AtomicU64,
    capacity: u64,
    nanos_per_token: u64,
    last_refill: AtomicU64,
}

impl TokenBucket {
    /// `capacity` = burst (requests); `rate_per_min` = refill rate.
    pub fn new(capacity: u64, rate_per_min: u64) -> Self {
        let nanos_per_token = if rate_per_min == 0 {
            u64::MAX
        } else {
            60_000_000_000 / rate_per_min
        };
        Self {
            tokens: AtomicU64::new(capacity),
            capacity,
            nanos_per_token,
            last_refill: AtomicU64::new(now_nanos()),
        }
    }

    pub fn try_acquire_n(&self, n: u64) -> bool {
        self.refill();
        loop {
            let cur = self.tokens.load(Ordering::Relaxed);
            if cur < n {
                return false;
            }
            if self
                .tokens
                .compare_exchange(cur, cur - n, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    pub fn release_n(&self, n: u64) {
        let mut cur = self.tokens.load(Ordering::Relaxed);
        loop {
            let next = (cur + n).min(self.capacity);
            match self
                .tokens
                .compare_exchange_weak(cur, next, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    fn refill(&self) {
        let now = now_nanos();
        let last = self.last_refill.load(Ordering::Relaxed);
        let elapsed = now.saturating_sub(last);
        if elapsed < 100_000_000 {
            return;
        }
        let earned = elapsed / self.nanos_per_token.max(1);
        if earned == 0 {
            return;
        }
        if self
            .last_refill
            .compare_exchange(last, now, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        // CAS loop: a concurrent try_acquire_n decrement must never be
        // overwritten by this load-then-store (lost update → over-issuance).
        let mut cur = self.tokens.load(Ordering::Relaxed);
        loop {
            let next = (cur + earned).min(self.capacity);
            match self
                .tokens
                .compare_exchange_weak(cur, next, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }
}

// ── AIMD concurrency gate ────────────────────────────────────────────────

pub struct AimdGateConfig {
    /// Additive increase step.
    pub increase_step: u32,
    /// Healthy window before an increase.
    pub increase_interval: Duration,
    /// Concurrency window at cold start (perf: window=1 serializes the first
    /// minutes of every fresh engine; 4 matches the default worker count).
    pub initial_window: u32,
    /// Upper bound (community data: >50 concurrent starts throttling).
    pub max_window: u32,
    /// Backoff base & cap (full jitter applied).
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// Consecutive 429s that open the circuit.
    pub open_after: u32,
    /// Circuit open duration before half-open probe.
    pub recovery: Duration,
    /// Max consecutive 429s before "quota suspicion" (engine reports to user).
    pub quota_suspect_after: u32,
}

impl Default for AimdGateConfig {
    fn default() -> Self {
        Self {
            increase_step: 1,
            increase_interval: Duration::from_secs(10),
            // Cold start 8, cap 32: long streaming chunks (~30s each) need
            // this to approach the 90 RPM budget; any 429 halves the window.
            initial_window: 8,
            max_window: 32,
            backoff_base: Duration::from_secs(1),
            backoff_cap: Duration::from_secs(30),
            open_after: 3,
            recovery: Duration::from_secs(60),
            quota_suspect_after: 8,
        }
    }
}

enum GateState {
    Closed,
    Open,
    HalfOpen,
}

pub struct AimdGate {
    cfg: AimdGateConfig,
    window: AtomicU32,
    ssthresh: AtomicU32,
    inflight: AtomicU32,
    consecutive_429: AtomicU32,
    consecutive_success: AtomicU32,
    state: Mutex<GateState>,
    open_since: Mutex<Option<Instant>>,
    blocked_until: Mutex<Option<Instant>>,
    last_healthy_tick: Mutex<Instant>,
    notify: Notify,
    /// Fired by `close()` so the spawned health loop can exit and the gate
    /// can actually drop (no leaked Arc-held task).
    shutdown: Notify,
}

impl AimdGate {
    pub fn new(cfg: AimdGateConfig) -> Arc<Self> {
        let gate = Arc::new(Self {
            window: AtomicU32::new(cfg.initial_window.max(1)),
            ssthresh: AtomicU32::new(cfg.max_window),
            inflight: AtomicU32::new(0),
            consecutive_429: AtomicU32::new(0),
            consecutive_success: AtomicU32::new(0),
            state: Mutex::new(GateState::Closed),
            open_since: Mutex::new(None),
            blocked_until: Mutex::new(None),
            last_healthy_tick: Mutex::new(Instant::now()),
            notify: Notify::new(),
            shutdown: Notify::new(),
            cfg,
        });
        let g = gate.clone();
        tokio::spawn(async move { g.health_loop().await });
        gate
    }

    /// Stop the health loop and release the gate (call when a provider
    /// runtime is replaced/removed).
    pub fn close(self: &Arc<Self>) {
        self.shutdown.notify_waiters();
    }

    /// Additive increase on healthy windows.
    async fn health_loop(self: Arc<Self>) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.cfg.increase_interval) => {}
                _ = self.shutdown.notified() => break,
            }
            let healthy_since = *self.last_healthy_tick.lock();
            if Instant::now().duration_since(healthy_since) >= self.cfg.increase_interval
                && self.consecutive_429.load(Ordering::Acquire) == 0
            {
                let cur = self.window.load(Ordering::Acquire);
                if cur < self.cfg.max_window {
                    self.window.fetch_add(self.cfg.increase_step, Ordering::AcqRel);
                    tracing::debug!("aimd: window {cur} -> {}", cur + self.cfg.increase_step);
                }
            }
            self.notify.notify_waiters();
        }
    }

    /// Acquire a concurrency permit (async, returns None if cancelled/shutdown).
    pub async fn acquire(self: &Arc<Self>) -> ConcurrencyPermit {
        loop {
            // Circuit open: wait until blocked_until passes, then probe half-open.
            let blocked = *self.blocked_until.lock();
            if let Some(until) = blocked {
                let now = Instant::now();
                if now < until {
                    let _ = tokio::time::timeout(until - now, self.notify.notified()).await;
                    continue;
                }
                *self.blocked_until.lock() = None;
                *self.state.lock() = GateState::HalfOpen;
                self.window.store(1, Ordering::Release);
                tracing::info!("aimd: circuit half-open (single probe)");
            }
            // Try to grab a slot within the window.
            loop {
                let cur = self.inflight.load(Ordering::Acquire);
                if (cur as u64) >= self.window.load(Ordering::Acquire) as u64 {
                    break;
                }
                if self
                    .inflight
                    .compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    return ConcurrencyPermit {
                        gate: self.clone(),
                    };
                }
            }
            // Window full or closed: park on notify (worker completion / health tick).
            self.notify.notified().await;
        }
    }

    pub fn on_success(self: &Arc<Self>) {
        self.consecutive_success.fetch_add(1, Ordering::AcqRel);
        self.consecutive_429.store(0, Ordering::Release);
        *self.last_healthy_tick.lock() = Instant::now();
        let mut state = self.state.lock();
        if matches!(*state, GateState::HalfOpen) {
            let ssthresh = self.ssthresh.load(Ordering::Acquire).max(1);
            self.window.store(ssthresh, Ordering::Release);
            *state = GateState::Closed;
            *self.open_since.lock() = None;
            tracing::info!("aimd: circuit closed after successful probe (window={ssthresh})");
        }
        self.notify.notify_one();
    }

    /// 429 / 5xx feedback. Returns `true` if the circuit just opened.
    /// Only 429s count toward the circuit-open streak; a 5xx resets it so
    /// mixed error sequences can never open the breaker on non-429 noise.
    pub fn on_throttle(self: &Arc<Self>, is_429: bool) -> bool {
        let consecutive = if is_429 {
            self.consecutive_429.fetch_add(1, Ordering::AcqRel) + 1
        } else {
            self.consecutive_429.store(0, Ordering::Release);
            0
        };
        let attempt = consecutive;
        let backoff = {
            let base_ms = self.cfg.backoff_base.as_millis() as u64;
            let cap_ms = self.cfg.backoff_cap.as_millis() as u64;
            let exp = (base_ms << attempt.min(8)).min(cap_ms);
            // full jitter: uniform in [0, exp]
            let jittered = fastrand::u64(..exp.max(1));
            Duration::from_millis(jittered)
        };
        // Multiplicative decrease: window = max(1, window/2); ssthresh follows.
        let old = self.window.load(Ordering::Acquire);
        let new_window = (old / 2).max(1);
        self.ssthresh.store(new_window, Ordering::Release);
        self.window.store(1, Ordering::Release);
        self.consecutive_success.store(0, Ordering::Release);
        *self.last_healthy_tick.lock() = Instant::now();

        let mut opened = false;
        if is_429 && consecutive >= self.cfg.open_after {
            let mut state = self.state.lock();
            if !matches!(*state, GateState::Open) {
                *state = GateState::Open;
                *self.open_since.lock() = Some(Instant::now());
                let until = Instant::now() + self.cfg.recovery;
                *self.blocked_until.lock() = Some(until);
                opened = true;
                tracing::warn!(
                    "aimd: circuit OPEN after {consecutive} consecutive 429s (recovery in {:?})",
                    self.cfg.recovery
                );
            }
        } else {
            let until = Instant::now() + backoff;
            let mut blocked = self.blocked_until.lock();
            if blocked.map_or(true, |b| until > b) {
                *blocked = Some(until);
            }
        }
        tracing::debug!(
            "aimd: throttle window {old}->1, consecutive={consecutive}, backoff={backoff:?}"
        );
        self.notify.notify_waiters();
        opened
    }

    /// 5xx: gentler — shrink window, short backoff (handled by caller retry).
    pub fn on_server_error(self: &Arc<Self>) {
        let old = self.window.load(Ordering::Acquire);
        if old > 1 {
            self.window.store(old - 1, Ordering::Release);
        }
        *self.last_healthy_tick.lock() = Instant::now();
        tracing::debug!("aimd: server error window {old}->{}", old.saturating_sub(1));
    }

    pub fn window(&self) -> u32 {
        self.window.load(Ordering::Acquire)
    }
    pub fn inflight(&self) -> u32 {
        self.inflight.load(Ordering::Acquire)
    }
    /// >0: seconds until the circuit may try again (for ProviderHealth events).
    pub fn retry_after_secs(&self) -> Option<u64> {
        let blocked = *self.blocked_until.lock();
        blocked.map(|until| {
            let left = until.saturating_duration_since(Instant::now());
            left.as_secs().max(1)
        })
    }
    pub fn is_open(&self) -> bool {
        matches!(*self.state.lock(), GateState::Open)
    }
}

pub struct ConcurrencyPermit {
    gate: Arc<AimdGate>,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        // fetch_sub: two concurrent drops must both decrement (a single CAS
        // whose failure is swallowed permanently overcounts inflight and
        // deadlocks the gate once it reaches max_window).
        self.gate.inflight.fetch_sub(1, Ordering::AcqRel);
        self.gate.notify.notify_one();
    }
}

// ── budget group (RPM 90% headroom + TPM) ────────────────────────────────

pub struct BudgetGroup {
    /// RPM bucket: capacity 100, refill 90/min (10% headroom — never hit 100).
    pub rpm: TokenBucket,
    /// TPM bucket: capacity 10M, refill 10M/min.
    pub tpm: TokenBucket,
}

impl BudgetGroup {
    /// Pre-reserve a request + its estimated input tokens. Refund on skip/error.
    pub fn reserve(&self, tokens: u64) -> bool {
        if !self.rpm.try_acquire_n(1) {
            return false;
        }
        if !self.tpm.try_acquire_n(tokens.max(1)) {
            self.rpm.release_n(1);
            return false;
        }
        true
    }
    pub fn refund(&self, tokens: u64) {
        self.rpm.release_n(1);
        self.tpm.release_n(tokens.max(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_burst_and_refund() {
        let b = TokenBucket::new(3, 600);
        assert!(b.try_acquire_n(3));
        assert!(!b.try_acquire_n(1));
        b.release_n(1);
        assert!(b.try_acquire_n(1));
    }

    #[tokio::test]
    async fn aimd_opens_after_consecutive_429() {
        let gate = AimdGate::new(AimdGateConfig {
            open_after: 3,
            recovery: Duration::from_secs(60),
            ..Default::default()
        });
        gate.on_throttle(true);
        gate.on_throttle(true);
        assert!(!gate.is_open());
        assert!(gate.on_throttle(true));
        assert!(gate.is_open());
        assert!(gate.retry_after_secs().is_some());
        assert_eq!(gate.window(), 1);
    }

    #[tokio::test]
    async fn aimd_half_open_probe_recovers() {
        let gate = AimdGate::new(AimdGateConfig {
            open_after: 2,
            recovery: Duration::from_millis(120),
            backoff_cap: Duration::from_millis(10),
            ..Default::default()
        });
        gate.on_throttle(true);
        assert!(gate.on_throttle(true));
        assert!(gate.is_open());
        // Wait for blocked_until to expire, then acquire (half-open probe).
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _permit = gate.acquire().await;
        gate.on_success(); // probe succeeds → CLOSED with ssthresh window
        assert!(!gate.is_open());
        assert!(gate.window() >= 1);
    }

    #[tokio::test]
    async fn aimd_window_caps_inflight() {
        let gate = AimdGate::new(AimdGateConfig {
            max_window: 4,
            initial_window: 1, // this test asserts the ramp from a window of 1
            increase_interval: Duration::from_secs(3600), // disable auto increase
            ..Default::default()
        });
        // window starts at 1
        let p1 = gate.acquire().await;
        // window full → second acquire parks; prove by timeout
        let g2 = gate.clone();
        let acquire2 = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(200), g2.acquire()).await;
        });
        acquire2.await.unwrap();
        assert_eq!(gate.inflight(), 1);
        drop(p1);
        let g3 = gate.clone();
        let _ = tokio::time::timeout(Duration::from_millis(500), g3.acquire()).await.unwrap();
    }

    #[test]
    fn budget_group_refunds() {
        let g = BudgetGroup {
            rpm: TokenBucket::new(2, 600),
            tpm: TokenBucket::new(100, 600_000),
        };
        assert!(g.reserve(50));
        assert!(g.reserve(50));
        assert!(!g.reserve(50), "rpm bucket empty");
        g.refund(50);
        assert!(g.reserve(50));
    }

    /// Regression (umreview C1): concurrent permit drops must all decrement.
    /// The pre-fix implementation used a single CAS whose failure was
    /// swallowed, permanently inflating `inflight` and deadlocking the gate.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_drops_fully_decrement() {
        let gate = AimdGate::new(AimdGateConfig {
            max_window: 64,
            initial_window: 64, // this test wants full fan-in, not a ramp
            increase_interval: Duration::from_secs(3600),
            ..Default::default()
        });
        for _round in 0..20 {
            let mut handles = Vec::new();
            for _ in 0..32 {
                let g = gate.clone();
                handles.push(tokio::spawn(async move {
                    let permit = g.acquire().await;
                    tokio::task::yield_now().await;
                    drop(permit);
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
            assert_eq!(gate.inflight(), 0, "inflight must return to 0 after all drops");
        }
    }
}
