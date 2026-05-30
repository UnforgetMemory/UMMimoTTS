//! Chunk processing queue.
//!
//! Orchestrates the MIMO API synthesis calls for individual chunks.
//! Workers pick pending chunks from the shared `chunks` table (ordered by
//! priority and creation time) and process them through the MIMO API.
//! Supports concurrency control, rate limiting, retry with back-off,
//! graceful shutdown, and crash recovery.

#![allow(dead_code)]

use crate::domain::chunk::ChunkStatus;
use crate::domain::events::DomainEvent;
use crate::shared::id::Id;
use crate::infra::cache::Cache;
use crate::infra::mimo::client::MimoClient;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::persistence::db::DbPool;
use crate::infra::persistence::task_repo::TaskRepo;
use crate::infra::queue::rate_limiter::TokenBucket;
use crate::shared::error::AppError;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Notify, Semaphore};
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
    /// When true, indicates recent 429 responses. Workers slow down their polling.
    rate_limited: Arc<AtomicBool>,
}

/// Maximum consecutive failures before pausing all chunk processing.
const MAX_CONSECUTIVE_FAILURES: u32 = 10;

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
        max_task_wait: Duration,
        cache_dir: std::path::PathBuf,
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
            rate_limited: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Wake up a worker to check for pending chunks.
    pub fn enqueue(&self, _chunk_id: &str, _task_id: &str) {
        self.notify.notify_one();
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
            let rate_limited = self.rate_limited.clone();

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
                    rate_limited,
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
    rate_limiter: Arc<TokenBucket>,
    token_budget: Arc<TokenBucket>,
    event_tx: broadcast::Sender<DomainEvent>,
    notify: Arc<Notify>,
    semaphore: Arc<Semaphore>,
    cancelled: Arc<AtomicBool>,
    cache_dir: std::path::PathBuf,
    consecutive_failures: Arc<AtomicU32>,
    paused: Arc<AtomicBool>,
    rate_limited: Arc<AtomicBool>,
) {
    info!("ChunkQueue worker {worker_id} started");

    loop {
        if cancelled.load(Ordering::Acquire) {
            info!("ChunkQueue worker {worker_id} shutting down");
            return;
        }

        // If circuit breaker is tripped, skip processing and wait
        if paused.load(Ordering::Acquire) {
            notify.notified().await;
            continue;
        }

        // If system-wide rate limiting is active, slow down polling
        if rate_limited.load(Ordering::Acquire) {
            warn!("worker {worker_id}: rate limiting active, sleeping 5s");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        // Check if rate-limited first
        if !rate_limiter.try_acquire() {
            notify.notified().await;
            continue;
        }

        // Acquire concurrency permit
        let _permit = match semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return,
        };

        // Pick a pending chunk
        let chunk = match chunk_repo.find_pending_prioritized(1) {
            Ok(mut chunks) => {
                if chunks.is_empty() {
                    drop(_permit);
                    notify.notified().await;
                    continue;
                }
                chunks.remove(0)
            }
            Err(e) => {
                error!("worker {worker_id}: fetch pending failed: {e}");
                drop(_permit);
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        let chunk_id = chunk.id.to_string();
        info!("worker {worker_id} picked chunk {chunk_id}");

        // Estimate token budget for this chunk (rough: 1 token ≈ 2 chars for Chinese text)
        let estimated_tokens = (chunk.text.len() as u64 / 2).max(1);
        if !token_budget.try_acquire_n(estimated_tokens) {
            warn!("worker {worker_id}: token budget exhausted, waiting for refill (need {estimated_tokens} tokens)");
            // Release semaphore permit before waiting
            drop(_permit);
            notify.notified().await;
            continue;
        }

        // Mark Processing
        if let Err(e) = chunk_repo.update_status(&chunk_id, &ChunkStatus::Processing) {
            error!("worker {worker_id}: mark_processing({chunk_id}) failed: {e}");
            drop(_permit);
            continue;
        }

        // Process the chunk
        let repo_c = chunk_repo.clone();
        let task_repo_c = task_repo.clone();
        let client_c = client.clone();
        let cache_c = cache.clone();
        let tx_c = event_tx.clone();
        let dir_c = cache_dir.clone();
        let chunk_id = chunk.id.to_string();
        let task_id = chunk.task_id.to_string();
        let seq = chunk.seq;
        let text = chunk.text.clone();
        let consecutive_failures_c = consecutive_failures.clone();
        let paused_c = paused.clone();
        let rate_limited_c = rate_limited.clone();

        tokio::spawn(async move {
            info!("chunk {chunk_id} processing started");

            const MAX_RETRIES: u32 = 10;
            let mut attempt: u32 = 0;
            let result = loop {
                let r = process_chunk(
                    repo_c.as_ref(),
                    &client_c,
                    task_repo_c.as_ref(),
                    cache_c.as_ref(),
                    &tx_c,
                    &chunk_id,
                    &task_id,
                    seq,
                    &text,
                    &dir_c,
                )
                .await;

                match &r {
                    Err(AppError::RateLimited) if attempt < MAX_RETRIES => {
                        let delay = std::cmp::min(1u64 << attempt, 30);
                        warn!(
                            "chunk {chunk_id} rate limited, retry {}/{MAX_RETRIES} after {}s",
                            attempt + 1,
                            delay
                        );
                        rate_limited_c.store(true, Ordering::Release);
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                        attempt += 1;
                    }
                    _ => break r,
                }
            };

            match result {
                Ok(()) => {
                    info!("chunk {chunk_id} completed successfully");
                    consecutive_failures_c.store(0, Ordering::Release);
                    rate_limited_c.store(false, Ordering::Release);
                }
                Err(e) => {
                    if matches!(&e, AppError::RateLimited) {
                        error!("chunk {chunk_id} rate limited after {MAX_RETRIES} retries, marking failed");
                        let _ = repo_c.mark_failed(&chunk_id, "Rate limited after 10 retries");
                        let _ = tx_c.send(DomainEvent::ChunkFailed {
                            chunk_id: Id::from_str(&chunk_id).unwrap_or_else(|_| Id::new()),
                            task_id: Id::from_str(&task_id).unwrap_or_else(|_| Id::new()),
                            seq,
                            error: "Rate limited after 10 retries".to_string(),
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
                    }
                }
            }
        });
    }
}

async fn process_chunk(
    chunk_repo: &dyn ChunkRepo,
    client: &MimoClient,
    task_repo: &dyn TaskRepo,
    cache: &Cache,
    event_tx: &broadcast::Sender<DomainEvent>,
    chunk_id: &str,
    task_id: &str,
    seq: i32,
    text: &str,
    cache_dir: &std::path::Path,
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

    // 2. Look up task to get voice and model
    let task = task_repo
        .find_by_id(task_id)?
        .ok_or_else(|| AppError::NotFound(format!("task {task_id} not found")))?;

    // 3. Call MIMO API
    let audio_bytes = client.synthesize(text, &task.voice, &task.model, task.speed).await?;

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
    use std::path::PathBuf;
    use std::time::Duration;

    fn setup_test_queue() -> (DbPool, Arc<dyn ChunkRepo>, ChunkQueue) {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
        let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
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
            Duration::from_millis(100),
            cache_dir,
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
