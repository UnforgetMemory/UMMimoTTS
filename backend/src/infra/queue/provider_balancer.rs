//! Provider load balancer with circuit breaker.
//!
//! Uses LeastConnections strategy: selects the provider with the lowest
//! `active_requests / rpm_capacity` ratio. Built-in circuit breaker
//! removes a provider after 5 consecutive failures; half-open retry
//! after 60 seconds.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Per-provider runtime state tracked by the balancer.
struct ProviderState {
    /// Currently active (in-flight) requests to this provider.
    active_requests: AtomicU32,
    /// Configured RPM capacity for this provider.
    rpm_capacity: u64,
    /// Consecutive failure count for circuit breaker.
    consecutive_failures: AtomicU32,
    /// Circuit breaker state: if Some(instant), provider is in open/half-open state.
    circuit_open_since: Option<Instant>,
}

/// Provider load balancer with circuit breaker support.
///
/// Thread-safe — all operations use interior mutability.
pub struct ProviderLoadBalancer {
    providers: RwLock<HashMap<String, ProviderState>>,
    /// Number of consecutive failures before opening circuit.
    failure_threshold: u32,
    /// Duration a provider stays in open state before half-open retry.
    recovery_timeout: Duration,
}

impl ProviderLoadBalancer {
    /// Create a new balancer with default circuit breaker settings.
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
        }
    }

    /// Create a balancer with custom circuit breaker settings.
    pub fn with_config(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            failure_threshold,
            recovery_timeout,
        }
    }

    /// Register a provider with its RPM capacity.
    pub fn add_provider(&self, provider_id: &str, rpm_capacity: u64) {
        let mut map = self.providers.write();
        map.insert(
            provider_id.to_string(),
            ProviderState {
                active_requests: AtomicU32::new(0),
                rpm_capacity,
                consecutive_failures: AtomicU32::new(0),
                circuit_open_since: None,
            },
        );
        info!("LoadBalancer: added provider '{provider_id}' with rpm_capacity={rpm_capacity}");
    }

    /// Remove a provider from the balancer.
    pub fn remove_provider(&self, provider_id: &str) {
        let mut map = self.providers.write();
        map.remove(provider_id);
    }

    /// Select the best available provider using LeastConnections strategy.
    ///
    /// Returns the provider_id with the lowest `active_requests / rpm_capacity` ratio
    /// among available providers. Returns `None` if all providers are circuit-broken.
    ///
    /// If a preferred provider_id is given and it's available, it is returned directly.
    pub fn select(&self, preferred: Option<&str>) -> Option<String> {
        let map = self.providers.read();

        // If preferred provider is specified and available, use it
        if let Some(pref_id) = preferred {
            if let Some(state) = map.get(pref_id) {
                if self.is_available(state) {
                    return Some(pref_id.to_string());
                }
            }
        }

        // LeastConnections: pick provider with lowest load ratio
        let now = Instant::now();
        let mut best: Option<(&str, f64)> = None;

        for (id, state) in map.iter() {
            if !self.is_available_at(state, now) {
                continue;
            }
            let active = state.active_requests.load(Ordering::Relaxed) as f64;
            let capacity = state.rpm_capacity.max(1) as f64;
            let ratio = active / capacity;

            if best.is_none() || ratio < best.unwrap().1 {
                best = Some((id.as_str(), ratio));
            }
        }

        best.map(|(id, _)| id.to_string())
    }

    /// Record the start of a request to a provider.
    pub fn on_request_start(&self, provider_id: &str) {
        let map = self.providers.read();
        if let Some(state) = map.get(provider_id) {
            state.active_requests.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record the end of a request (success or failure).
    pub fn on_request_end(&self, provider_id: &str) {
        let map = self.providers.read();
        if let Some(state) = map.get(provider_id) {
            // Atomic decrement with saturation at 0
            let _ = state.active_requests.fetch_update(
                Ordering::Release, Ordering::Relaxed,
                |current| Some(current.saturating_sub(1)),
            );
        }
    }

    /// Record a successful request to a provider.
    /// Resets failure count AND closes circuit if it was half-open.
    pub fn on_success(&self, provider_id: &str) {
        let mut map = self.providers.write();
        if let Some(state) = map.get_mut(provider_id) {
            let prev = state.consecutive_failures.swap(0, Ordering::Relaxed);
            if prev > 0 {
                info!("LoadBalancer: provider '{provider_id}' recovered after {prev} failures");
            }
            // Close circuit on successful half-open probe
            if state.circuit_open_since.is_some() {
                state.circuit_open_since = None;
                info!("LoadBalancer: circuit CLOSED for provider '{provider_id}' after successful probe");
            }
        }
    }

    /// Record a failed request to a provider. Opens circuit after threshold.
    pub fn on_failure(&self, provider_id: &str) {
        let mut map = self.providers.write();
        if let Some(state) = map.get_mut(provider_id) {
            let count = state.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.failure_threshold && state.circuit_open_since.is_none() {
                state.circuit_open_since = Some(Instant::now());
                warn!(
                    "LoadBalancer: circuit OPEN for provider '{provider_id}' \
                     after {count} consecutive failures (recovery in {:?})",
                    self.recovery_timeout
                );
            }
        }
    }

    /// Check if a provider state is currently available (circuit not open).
    fn is_available(&self, state: &ProviderState) -> bool {
        self.is_available_at(state, Instant::now())
    }

    /// Check availability at a specific instant.
    fn is_available_at(&self, state: &ProviderState, now: Instant) -> bool {
        match state.circuit_open_since {
            None => true,
            Some(opened_at) => {
                // Half-open: allow retry after recovery timeout
                if now.duration_since(opened_at) >= self.recovery_timeout {
                    true // half-open state — allow one probe request
                } else {
                    false // circuit still open
                }
            }
        }
    }

    /// Reset circuit breaker for a provider (e.g., after successful half-open probe).
    pub fn reset_circuit(&self, provider_id: &str) {
        let mut map = self.providers.write();
        if let Some(state) = map.get_mut(provider_id) {
            state.circuit_open_since = None;
            state.consecutive_failures.store(0, Ordering::Relaxed);
            info!("LoadBalancer: circuit RESET for provider '{provider_id}'");
        }
    }

    /// Get current active request count for a provider.
    pub fn active_requests(&self, provider_id: &str) -> u32 {
        let map = self.providers.read();
        map.get(provider_id)
            .map(|s| s.active_requests.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get list of all registered provider IDs.
    pub fn provider_ids(&self) -> Vec<String> {
        let map = self.providers.read();
        map.keys().cloned().collect()
    }
}

impl Default for ProviderLoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_returns_added_provider() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);
        assert_eq!(lb.select(None), Some("p1".to_string()));
    }

    #[test]
    fn test_select_least_connections() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);
        lb.add_provider("p2", 100);

        // p1 has 3 active, p2 has 1 active
        for _ in 0..3 {
            lb.on_request_start("p1");
        }
        lb.on_request_start("p2");

        assert_eq!(lb.select(None), Some("p2".to_string()));
    }

    #[test]
    fn test_select_preferred_provider() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);
        lb.add_provider("p2", 100);

        lb.on_request_start("p1");
        // Even though p1 has more load, preferred returns it
        assert_eq!(lb.select(Some("p1")), Some("p1".to_string()));
    }

    #[test]
    fn test_circuit_breaker_opens() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);
        lb.add_provider("p2", 100);

        // 5 failures on p1 should open circuit
        for _ in 0..5 {
            lb.on_failure("p1");
        }

        // p1 should be skipped
        assert_eq!(lb.select(None), Some("p2".to_string()));
    }

    #[test]
    fn test_circuit_breaker_half_open() {
        let lb = ProviderLoadBalancer::with_config(3, Duration::from_millis(50));
        lb.add_provider("p1", 100);

        for _ in 0..3 {
            lb.on_failure("p1");
        }

        // Circuit is open — no providers available
        assert_eq!(lb.select(None), None);

        // Wait for recovery timeout
        std::thread::sleep(Duration::from_millis(60));

        // Half-open: p1 should be selectable again
        assert_eq!(lb.select(None), Some("p1".to_string()));
    }

    #[test]
    fn test_success_resets_failures() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);

        for _ in 0..4 {
            lb.on_failure("p1");
        }
        lb.on_success("p1");

        // Failures reset, so p1 is still available
        assert_eq!(lb.select(None), Some("p1".to_string()));
    }

    #[test]
    fn test_active_requests_tracking() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);

        lb.on_request_start("p1");
        lb.on_request_start("p1");
        assert_eq!(lb.active_requests("p1"), 2);

        lb.on_request_end("p1");
        assert_eq!(lb.active_requests("p1"), 1);
    }

    #[test]
    fn test_remove_provider() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("p1", 100);
        lb.add_provider("p2", 100);
        lb.remove_provider("p1");

        assert_eq!(lb.select(None), Some("p2".to_string()));
    }

    #[test]
    fn test_empty_balancer_returns_none() {
        let lb = ProviderLoadBalancer::new();
        assert_eq!(lb.select(None), None);
    }

    #[test]
    fn test_capacity_weighted_selection() {
        let lb = ProviderLoadBalancer::new();
        lb.add_provider("small", 50);
        lb.add_provider("large", 200);

        // Both have 10 active requests
        for _ in 0..10 {
            lb.on_request_start("small");
            lb.on_request_start("large");
        }

        // small: 10/50 = 0.2, large: 10/200 = 0.05 → large should be selected
        assert_eq!(lb.select(None), Some("large".to_string()));
    }
}
