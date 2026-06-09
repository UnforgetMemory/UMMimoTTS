//! Chunk processing queue.
//!
//! Orchestrates the MIMO API synthesis calls for individual chunks.
//! Workers pick pending chunks from the shared `chunks` table (ordered by
//! priority and creation time) and process them through the MIMO API.
//! Supports concurrency control, rate limiting, retry with back-off,
//! graceful shutdown, and crash recovery.

#![allow(dead_code)]

use crate::domain::events::DomainEvent;
use crate::domain::task::TaskStatus;
use crate::shared::id::Id;
use crate::infra::cache::Cache;
use crate::infra::mimo::client::MimoClient;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::persistence::db::DbPool;
use crate::infra::persistence::task_repo::TaskRepo;
use crate::infra::persistence::provider_repo::ProviderRepo;
use crate::infra::queue::rate_limiter::{TokenBucket, ProviderRateLimiterMap};
use crate::infra::queue::provider_balancer::ProviderLoadBalancer;
use crate::shared::error::AppError;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, Notify, Semaphore};
use std::collections::HashSet;
use tracing::{error, info, warn};

/// Manages concurrent synthesis of audio chunks via the MIMO API.
pub struct ChunkQueue {
    pool: DbPool,
    chunk_repo: Arc<dyn ChunkRepo>,
    task_repo: Arc<dyn TaskRepo>,
    client: Arc<MimoClient>,
    cache: Arc<Cache>,
    rate_limiter: Arc<TokenBucket>,
    token_budget: Arc<TokenBucket>,
    event_tx: broadcast::Sender<DomainEvent>,
    notify: Arc<Notify>,
    cancelled: Arc<AtomicBool>,
    semaphore: Arc<Semaphore>,
    pub max_concurrent: usize,
    max_task_wait: Duration,
    cache_dir: std::path::PathBuf,
    /// Consecutive chunk processing failures across all workers.
    /// Reset to 0 on any success; when exceeding `MAX_CONSECUTIVE_FAILURES`,
    /// all workers pause processing.
    consecutive_failures: Arc<AtomicU32>,
    /// When true, workers skip processing and go back to waiting.
    paused: Arc<AtomicBool>,
    /// Task-level concurrency: tracks which tasks currently have chunks in-flight.
    active_tasks: Arc<Mutex<HashSet<String>>>,
    /// Semaphore limiting how many tasks can be actively processing concurrently.
    task_semaphore: Arc<Semaphore>,
    /// Maximum number of tasks that can be actively processing at once.
    max_active_tasks: usize,
    /// Provider repo for resolving MIMO API credentials per chunk.
    provider_repo: Arc<dyn ProviderRepo>,
    /// Per-provider rate limiter map (each provider has independent RPM/TPM quota).
    provider_rate_limiters: Arc<ProviderRateLimiterMap>,
    /// Load balancer for provider selection (LeastConnections + circuit breaker).
    load_balancer: Arc<ProviderLoadBalancer>,
}

/// Maximum consecutive failures before pausing all chunk processing.
const MAX_CONSECUTIVE_FAILURES: u32 = 20;

impl ChunkQueue {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: DbPool,
        chunk_repo: Arc<dyn ChunkRepo>,
        task_repo: Arc<dyn TaskRepo>,
        client: Arc<MimoClient>,
        cache: Arc<Cache>,
        rate_limiter: Arc<TokenBucket>,
        token_budget: Arc<TokenBucket>,
        event_tx: broadcast::Sender<DomainEvent>,
        max_concurrent: usize,
        max_active_tasks: usize,
        max_task_wait: Duration,
        cache_dir: std::path::PathBuf,
        provider_repo: Arc<dyn ProviderRepo>,
        provider_rate_limiters: Arc<ProviderRateLimiterMap>,
        load_balancer: Arc<ProviderLoadBalancer>,
    ) -> Self {
        Self {
            pool,
            chunk_repo,
            task_repo,
            client,
            cache,
            rate_limiter,
            token_budget,
            event_tx,
            notify: Arc::new(Notify::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            max_task_wait,
            cache_dir,
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            paused: Arc::new(AtomicBool::new(false)),
            active_tasks: Arc::new(Mutex::new(HashSet::new())),
            task_semaphore: Arc::new(Semaphore::new(max_active_tasks)),
            max_active_tasks,
            provider_repo,
            provider_rate_limiters,
            load_balancer,
        }
    }

    /// Wake up a worker to check for pending chunks.
    pub fn enqueue(&self, _chunk_id: &str, _task_id: &str) {
        self.notify.notify_one();
    }

    /// Wake all workers — used to expedite priority-boosted tasks.
    pub fn wake_all(&self) {
        self.notify.notify_waiters();
    }

    /// Spawn `max_concurrent` background worker tasks.
    pub fn run_workers(&self) {
        for i in 0..self.max_concurrent {
            let chunk_repo = self.chunk_repo.clone();
            let task_repo = self.task_repo.clone();
            let client = self.client.clone();
            let cache = self.cache.clone();
            let rate_limiter = self.rate_limiter.clone();
            let token_budget = self.token_budget.clone();
            let event_tx = self.event_tx.clone();
            let notify = self.notify.clone();
            let semaphore = self.semaphore.clone();
            let cancel = self.cancelled.clone();
            let cache_dir = self.cache_dir.clone();
            let consecutive_failures = self.consecutive_failures.clone();
            let paused = self.paused.clone();
            let active_tasks = self.active_tasks.clone();
            let task_semaphore = self.task_semaphore.clone();
            let provider_repo = self.provider_repo.clone();
            let provider_rate_limiters = self.provider_rate_limiters.clone();
            let load_balancer = self.load_balancer.clone();

            let max_task_wait = self.max_task_wait;

            tokio::spawn(async move {
                worker_loop(
                    i,
                    chunk_repo,
                    task_repo,
                    client,
                    cache,
                    rate_limiter,
                    token_budget,
                    event_tx,
                    notify,
                    semaphore,
                    cancel,
                    cache_dir,
                    consecutive_failures,
                    paused,
                    active_tasks,
                    task_semaphore,
                    provider_repo,
                    provider_rate_limiters,
                    load_balancer,
                    max_task_wait,
                )
                .await;
            });
        }
    }

    /// Signal all workers to shut down gracefully.
    pub fn shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        for _ in 0..self.max_concurrent {
            self.notify.notify_one();
        }
    }
}

async fn worker_loop(
    worker_id: usize,
    chunk_repo: Arc<dyn ChunkRepo>,
    task_repo: Arc<dyn TaskRepo>,
    client: Arc<MimoClient>,
    cache: Arc<Cache>,
    _rate_limiter: Arc<TokenBucket>,
    _token_budget: Arc<TokenBucket>,
    event_tx: broadcast::Sender<DomainEvent>,
    notify: Arc<Notify>,
    semaphore: Arc<Semaphore>,
    cancelled: Arc<AtomicBool>,
    cache_dir: std::path::PathBuf,
    consecutive_failures: Arc<AtomicU32>,
    paused: Arc<AtomicBool>,
    active_tasks: Arc<Mutex<HashSet<String>>>,
    task_semaphore: Arc<Semaphore>,
    provider_repo: Arc<dyn ProviderRepo>,
    provider_rate_limiters: Arc<ProviderRateLimiterMap>,
    load_balancer: Arc<ProviderLoadBalancer>,
    max_task_wait: Duration,
) {
    info!("ChunkQueue worker {worker_id} started");

    loop {
        if cancelled.load(Ordering::Acquire) {
            info!("ChunkQueue worker {worker_id} shutting down");
            return;
        }

        // If circuit breaker is tripped, wait with timeout before auto-resetting
        if paused.load(Ordering::Acquire) {
            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    warn!("ChunkQueue worker {worker_id}: circuit breaker auto-reset after 30s timeout");
                    paused.store(false, Ordering::Release);
                    consecutive_failures.store(0, Ordering::Release);
                    // Exit degraded mode for all providers when circuit breaker resets
                    for pid in provider_rate_limiters.provider_ids() {
                        provider_rate_limiters.exit_degraded_mode(&pid);
                    }
                }
            }
            continue;
        }

        // ── Step 1: Discover work (JOIN query — chunk + task info in one DB call) ──
        let cti = match chunk_repo.find_pending_with_task(10) {
            Ok(mut results) => {
                if results.is_empty() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
                results.remove(0)
            }
            Err(e) => {
                error!("worker {worker_id}: fetch pending failed: {e}");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        let chunk = cti.chunk;
        let chunk_id = chunk.id.to_string();
        let task_id_str = chunk.task_id.to_string();

        // ── Step 2: Check parent task status (from JOIN result — no extra DB call) ──
        if matches!(cti.task_status, TaskStatus::Cancelled | TaskStatus::Failed | TaskStatus::Done) {
            info!("worker {worker_id}: marking chunk {chunk_id} as Failed, parent task {task_id_str} is {:?}", cti.task_status);
            let _ = chunk_repo.mark_failed(&chunk_id, &format!("Parent task is {:?}", cti.task_status));
            continue;
        }
        if matches!(cti.task_status, TaskStatus::Paused) {
            info!("worker {worker_id}: skipping chunk {chunk_id}, parent task {task_id_str} is paused");
            continue;
        }

        // ── Step 3: Per-provider rate limiting with load balancer ──
        let provider_id = if let Some(ref pid) = cti.task_provider_id {
            // Task has an explicit provider — use it (balancer prefers it)
            pid.clone()
        } else {
            // No explicit provider — let the load balancer pick the best available
            load_balancer.select(None).unwrap_or_else(|| {
                provider_repo.find_default()
                    .ok()
                    .flatten()
                    .map(|p| p.id)
                    .unwrap_or_else(|| "default".to_string())
            })
        };
        load_balancer.on_request_start(&provider_id);
        let provider_limiter = provider_rate_limiters.get_or_create(&provider_id);

        // Acquire RPM token from the provider-specific bucket
        provider_limiter.acquire_rpm().await;

        // ── Step 4: Acquire chunk-level concurrency permit ──
        let _permit = match semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return,
        };

        // ── Task-level concurrency gate ──
        // Check if this task already has chunks in-flight.
        let is_new_task = {
            let active = active_tasks.lock().await;
            !active.contains(&task_id_str)
        };

        if is_new_task {
            // New task — try to acquire task semaphore permit NON-BLOCKING.
            let task_permit = match task_semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    // Can't start this task yet — release permits and try later.
                    provider_limiter.release_rpm(1);
                    load_balancer.on_request_end(&provider_id);
                    drop(_permit);
                    notify.notify_one();
                    continue;
                }
            };
            // Mark task as active
            {
                let mut active = active_tasks.lock().await;
                active.insert(task_id_str.clone());
            }
            info!("worker {worker_id}: new task {task_id_str} acquired task slot (active tasks tracked)");

            // Spawn a background task that releases the permit when all chunks for this task are done.
            // Includes a max_task_wait timeout to prevent stuck tasks from blocking the queue.
            let active_tasks_release = active_tasks.clone();
            let chunk_repo_release = chunk_repo.clone();
            let task_id_release = task_id_str.clone();
            let notify_release = notify.clone();
            tokio::spawn(async move {
                let deadline = tokio::time::Instant::now() + max_task_wait;
                // Wait until no pending/processing chunks remain for this task.
                // Poll every 2 seconds, with an upper bound of max_task_wait.
                loop {
                    let poll = tokio::time::sleep(Duration::from_secs(5));
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => {
                            warn!("task {task_id_release} max_task_wait ({max_task_wait:?}) expired — releasing permit early");
                            break;
                        }
                        _ = poll => {
                            let (_total, _done, _failed, pending, processing) = chunk_repo_release
                                .count_by_task_aggregated(&task_id_release)
                                .unwrap_or((0, 0, 0, 0, 0));
                            if pending == 0 && processing == 0 {
                                break;
                            }
                        }
                    }
                }
                // All chunks done (or timeout) — remove from active set and release permit.
                {
                    let mut active = active_tasks_release.lock().await;
                    active.remove(&task_id_release);
                }
                drop(task_permit); // releases the semaphore permit
                notify_release.notify_one(); // wake a worker to pick up the freed slot
                info!("task {task_id_release} released task slot");
            });
        }

        info!("worker {worker_id} picked chunk {chunk_id} (provider={provider_id})");

        // Estimate token budget for this chunk (rough: 1 token ≈ 2 chars for Chinese text)
        let estimated_tokens = (chunk.text.len() as u64 / 2).max(1);
        if !provider_limiter.try_acquire_tpm(estimated_tokens) {
            warn!("worker {worker_id}: token budget exhausted for provider {provider_id}, waiting for refill (need {estimated_tokens} tokens)");
            provider_limiter.release_rpm(1);
            load_balancer.on_request_end(&provider_id);
            drop(_permit);
            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            continue;
        }

        // Mark chunk Processing with optimistic locking.
        // Uses WHERE status='pending' to prevent double-processing.
        match chunk_repo.try_mark_processing(&chunk_id) {
            Ok(true) => {} // we got it
            Ok(false) => {
                // Another worker already claimed this chunk — refund and move on
                provider_limiter.release_rpm(1);
                provider_limiter.release_tpm(estimated_tokens);
                load_balancer.on_request_end(&provider_id);
                drop(_permit);
                continue;
            }
            Err(e) => {
                error!("worker {worker_id}: try_mark_processing({chunk_id}) failed: {e}");
                provider_limiter.release_rpm(1);
                provider_limiter.release_tpm(estimated_tokens);
                load_balancer.on_request_end(&provider_id);
                drop(_permit);
                continue;
            }
        }

        // Transition task to Processing if it's still in Queued state.
        // Tasks stay Queued after enqueue; first chunk pickup → Processing.
        // We already have the task info from the JOIN query — no extra DB query needed.
        if cti.task_status == TaskStatus::Queued {
            if let Err(e) = task_repo.update_status(&task_id_str, &TaskStatus::Processing) {
                warn!("worker {worker_id}: failed to transition task {task_id_str} to Processing: {e}");
            } else {
                let _ = event_tx.send(DomainEvent::TaskStatusChanged {
                    task_id: chunk.task_id.clone(),
                    batch_id: cti.task_batch_id.clone(),
                    status: "processing".to_string(),
                });
            }
        }

        // Process the chunk
        let repo_c = chunk_repo.clone();
        let client_c = client.clone();
        let cache_c = cache.clone();
        let tx_c = event_tx.clone();
        let dir_c = cache_dir.clone();
        let provider_repo_c = provider_repo.clone();
        let chunk_id = chunk.id.to_string();
        let task_id = chunk.task_id.to_string();
        let seq = chunk.seq;
        let text = chunk.text.clone();
        let task_voice = cti.task_voice;
        let task_model = cti.task_model;
        let task_speed = cti.task_speed;
        let task_provider_id = cti.task_provider_id;
        let consecutive_failures_c = consecutive_failures.clone();
        let paused_c = paused.clone();
        let load_balancer_c = load_balancer.clone();
        let provider_rl_c = provider_rate_limiters.clone();
        let selected_provider_id = provider_id.clone();
        tokio::spawn(async move {
            info!("chunk {chunk_id} processing started");

            const MAX_RETRIES: u32 = 10;
            let mut attempt: u32 = 0;
            let mut consecutive_429: u32 = 0;
            let mut consecutive_net_err: u32 = 0;
            let result = loop {
                let r = process_chunk(
                    repo_c.as_ref(),
                    &client_c,
                    cache_c.as_ref(),
                    &tx_c,
                    provider_repo_c.as_ref(),
                    &chunk_id,
                    &task_id,
                    seq,
                    &text,
                    &dir_c,
                    &task_voice,
                    &task_model,
                    task_speed,
                    task_provider_id.as_deref(),
                )
                .await;

                match &r {
                    Err(e) if attempt < MAX_RETRIES => {
                        let is_rate_limited = matches!(e, AppError::RateLimited);
                        let is_server_overload = matches!(e, AppError::ServerOverload(_));
                        // Both rate limiting and server overload are retryable with same strategy
                        let is_throttle = is_rate_limited || is_server_overload;
                        let err_str = e.to_string();
                        let is_network_error = err_str.contains("error sending request")
                            || err_str.contains("connection")
                            || err_str.contains("dns")
                            || err_str.contains("timeout");

                        let (max_retries, backoff_cap) = if is_throttle || is_network_error {
                            (MAX_RETRIES, 30u64)
                        } else {
                            (MAX_RETRIES, 30u64)
                        };

                        if attempt >= max_retries {
                            warn!("chunk {chunk_id} exceeded {max_retries} retries for {:?}, giving up", e);
                            break r;
                        }

                        // ── 429 / 5xx degradation trigger ──
                        if is_throttle {
                            consecutive_429 += 1;
                            consecutive_net_err = 0;
                            // Enter degraded mode: 10 RPM / 500K TPM for 60s
                            provider_rl_c.enter_degraded_mode(
                                &selected_provider_id, 10, 500_000,
                                Duration::from_secs(60),
                            );
                            // 3 consecutive throttle errors → trip circuit breaker
                            if consecutive_429 >= 3 {
                                let kind = if is_rate_limited { "429" } else { "5xx" };
                                warn!("chunk {chunk_id}: {consecutive_429} consecutive {kind}s — tripping circuit breaker");
                                paused_c.store(true, Ordering::Release);
                            }
                        } else if is_network_error {
                            consecutive_net_err += 1;
                            consecutive_429 = 0;
                            // 2 consecutive network errors → enter degraded mode (120s)
                            if consecutive_net_err >= 2 {
                                provider_rl_c.enter_degraded_mode(
                                    &selected_provider_id, 5, 200_000,
                                    Duration::from_secs(120),
                                );
                            }
                        } else {
                            consecutive_429 = 0;
                            consecutive_net_err = 0;
                        }

                        // ── Exponential backoff with jitter ──
                        let base_delay = std::cmp::min(1u64 << attempt, backoff_cap);
                        let jitter_range = std::cmp::max(1, base_delay / 3);
                        // Simple deterministic jitter using chunk_id hash
                        let hash = chunk_id.as_bytes().iter()
                            .fold(attempt as u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));
                        let jitter = (hash % (jitter_range * 2 + 1)) as i64 - jitter_range as i64;
                        let delay = std::cmp::max(1, (base_delay as i64 + jitter) as u64);

                        let kind = if is_rate_limited { "rate_limit" } else if is_server_overload { "server_overload" } else if is_network_error { "network" } else { "other" };
                        warn!(
                            "chunk {chunk_id} failed ({kind}), retry {}/{max_retries} after {delay}s: {e}",
                            attempt + 1,
                        );
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                        attempt += 1;
                    }
                    _ => break r,
                }
            };

            // Update load balancer tracking
            load_balancer_c.on_request_end(&selected_provider_id);

            match result {
                Ok(()) => {
                    info!("chunk {chunk_id} completed successfully");
                    consecutive_failures_c.store(0, Ordering::Release);
                    load_balancer_c.on_success(&selected_provider_id);
                }
                Err(e) => {
                    load_balancer_c.on_failure(&selected_provider_id);

                    // ── Circuit breaker feedback to rate limiter ──
                    if matches!(&e, AppError::RateLimited | AppError::ServerOverload(_)) {
                        // Ensure degraded mode is active for throttled/overloaded provider
                        provider_rl_c.enter_degraded_mode(
                            &selected_provider_id, 5, 200_000,
                            Duration::from_secs(120),
                        );
                    }

                    if matches!(&e, AppError::RateLimited | AppError::ServerOverload(_)) {
                        let reason = if matches!(&e, AppError::RateLimited) {
                            "Rate limited (429)"
                        } else if e.to_string().contains("TCP/connect") {
                            "TCP connection refused"
                        } else {
                            "Server overload (5xx)"
                        };
                        let msg = format!("{reason} after {MAX_RETRIES} retries");
                        error!("chunk {chunk_id}: {msg}");
                        let _ = repo_c.mark_failed(&chunk_id, &msg);
                        let _ = tx_c.send(DomainEvent::ChunkFailed {
                            chunk_id: Id::from_str(&chunk_id).unwrap_or_else(|_| Id::new()),
                            task_id: Id::from_str(&task_id).unwrap_or_else(|_| Id::new()),
                            seq,
                            error: msg,
                            retry_count: 0,
                        });
                    } else {
                        error!("chunk {chunk_id} processing failed: {e}");
                        let _ = repo_c.mark_failed(&chunk_id, &e.to_string());
                        let _ = tx_c.send(DomainEvent::ChunkFailed {
                            chunk_id: Id::from_str(&chunk_id).unwrap_or_else(|_| Id::new()),
                            task_id: Id::from_str(&task_id).unwrap_or_else(|_| Id::new()),
                            seq,
                            error: e.to_string(),
                            retry_count: 0,
                        });
                    }

                    // Circuit breaker: trip on too many consecutive failures
                    let prev = consecutive_failures_c.fetch_add(1, Ordering::AcqRel);
                    if prev + 1 >= MAX_CONSECUTIVE_FAILURES {
                        error!(
                            "chunk queue circuit breaker tripped after {} consecutive failures",
                            prev + 1
                        );
                        paused_c.store(true, Ordering::Release);
                        // Aggressive degraded mode when circuit breaker trips
                        provider_rl_c.enter_degraded_mode(
                            &selected_provider_id, 3, 100_000,
                            Duration::from_secs(180),
                        );
                    }
                }
            }
        });
        // Wake another worker to pick up the next pending chunk immediately
        notify.notify_one();
    }
}

async fn process_chunk(
    chunk_repo: &dyn ChunkRepo,
    client: &MimoClient,
    cache: &Cache,
    event_tx: &broadcast::Sender<DomainEvent>,
    provider_repo: &dyn ProviderRepo,
    chunk_id: &str,
    task_id: &str,
    seq: i32,
    text: &str,
    cache_dir: &std::path::Path,
    task_voice: &str,
    task_model: &str,
    task_speed: f64,
    task_provider_id: Option<&str>,
) -> Result<(), AppError> {
    // 1. Check cache
    let cache_key = format!("chunk_{chunk_id}");
    if let Some(cached_path) = cache.get(&cache_key) {
        let path_str = String::from_utf8_lossy(&cached_path).to_string();
        chunk_repo.mark_done(chunk_id, &path_str, 0.5)?;
        let _ = event_tx.send(DomainEvent::ChunkCompleted {
            chunk_id: crate::shared::id::Id::from_str(chunk_id).unwrap_or_else(|_| crate::shared::id::Id::new()),
            task_id: crate::shared::id::Id::from_str(task_id).unwrap_or_else(|_| crate::shared::id::Id::new()),
            seq,
            audio_path: path_str,
            duration: 0.5,
        });
        return Ok(());
    }

    // 2. Resolve provider (task-level → default) — no task DB query needed
    let provider = if let Some(pid) = task_provider_id {
        provider_repo.find_by_id(pid)?
    } else {
        provider_repo.find_default()?
    };
    let provider = provider.ok_or_else(|| AppError::Internal("No configured MIMO provider found".into()))?;
    if !provider.is_configured {
        return Err(AppError::Internal(format!(
            "Provider '{}' is not configured (no API key). Go to Settings to set it up.",
            provider.name
        )));
    }

    // 3. Call MIMO API
    let audio_bytes = client.synthesize(
        text, task_voice, task_model, task_speed,
        &provider.api_key, &provider.base_url,
    ).await?;

    // 3. Write to disk
    let file_name = format!("chunk_{task_id}_{seq}.wav");
    let out_path = cache_dir.join(&file_name);
    tokio::fs::write(&out_path, &audio_bytes).await?;

    // 4. Store in cache
    let path_str = out_path.to_string_lossy().to_string();
    let _ = cache.put(&cache_key, path_str.as_bytes().to_vec());

    // 5. Mark chunk Done
    chunk_repo.mark_done(chunk_id, &path_str, 0.5)?;

    // 6. Publish event
    let _ = event_tx.send(DomainEvent::ChunkCompleted {
        chunk_id: crate::shared::id::Id::from_str(chunk_id).unwrap_or_else(|_| crate::shared::id::Id::new()),
        task_id: crate::shared::id::Id::from_str(task_id).unwrap_or_else(|_| crate::shared::id::Id::new()),
        seq,
        audio_path: path_str,
        duration: 0.5,
    });

    Ok(())
}

/// Recover chunks that were left in Processing state after a crash.
pub async fn recover(chunk_repo: &dyn ChunkRepo) -> Result<usize, AppError> {
    chunk_repo.reset_processing_to_pending()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::cache::Cache;
    use crate::infra::mimo::client::MimoClient;
    use crate::infra::persistence::chunk_repo::SqliteChunkRepo;
    use crate::infra::persistence::db::create_test_pool;
    use crate::infra::persistence::migrate::run_migrations;
    use crate::infra::persistence::task_repo::SqliteTaskRepo;
    use crate::infra::persistence::provider_repo::SqliteProviderRepo;
    use std::path::PathBuf;
    use std::time::Duration;

    fn setup_test_queue() -> (DbPool, Arc<dyn ChunkRepo>, ChunkQueue) {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
        let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
        let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
        let _ = provider_repo.update_api_key("xiaomi", "test-key");
        let client = Arc::new(MimoClient::new("test-key", "http://localhost:30231"));
        let cache = Arc::new(Cache::new(
            PathBuf::from("data/test_cache"),
            Duration::from_secs(300),
            100,
        ));
        let rate_limiter = Arc::new(TokenBucket::new(1000));
        let token_budget = Arc::new(TokenBucket::new(1_000_000));
        let (event_tx, _) = broadcast::channel(512);
        let cache_dir = PathBuf::from("data/test_cache");
        let provider_rate_limiters = Arc::new(ProviderRateLimiterMap::new(1000, 10_000_000, 10));
        let load_balancer = Arc::new(ProviderLoadBalancer::new());

        let queue = ChunkQueue::new(
            pool.clone(),
            chunk_repo.clone(),
            task_repo,
            client,
            cache,
            rate_limiter,
            token_budget,
            event_tx,
            2,
            20,
            Duration::from_millis(100),
            cache_dir,
            provider_repo,
            provider_rate_limiters,
            load_balancer,
        );
        (pool, chunk_repo, queue)
    }

    #[test]
    fn test_chunk_queue_creation() {
        let (_, _, queue) = setup_test_queue();
        assert_eq!(queue.max_concurrent, 2);
    }

    #[actix_rt::test]
    async fn test_chunk_queue_shutdown() {
        let (_, _, queue) = setup_test_queue();
        queue.shutdown();
        assert!(queue.cancelled.load(Ordering::Acquire));
    }
}
