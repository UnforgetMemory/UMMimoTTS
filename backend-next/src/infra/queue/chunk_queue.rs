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
use crate::infra::cache::Cache;
use crate::infra::mimo::client::MimoClient;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::persistence::db::DbPool;
use crate::infra::queue::rate_limiter::TokenBucket;
use crate::shared::error::AppError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Notify, Semaphore};
use tracing::{error, info};

/// Manages concurrent synthesis of audio chunks via the MIMO API.
pub struct ChunkQueue {
    pool: DbPool,
    chunk_repo: Arc<dyn ChunkRepo>,
    client: Arc<MimoClient>,
    cache: Arc<Cache>,
    rate_limiter: Arc<TokenBucket>,
    event_tx: broadcast::Sender<DomainEvent>,
    notify: Arc<Notify>,
    cancelled: Arc<AtomicBool>,
    semaphore: Arc<Semaphore>,
    pub max_concurrent: usize,
    max_task_wait: Duration,
    cache_dir: std::path::PathBuf,
}

impl ChunkQueue {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: DbPool,
        chunk_repo: Arc<dyn ChunkRepo>,
        client: Arc<MimoClient>,
        cache: Arc<Cache>,
        rate_limiter: Arc<TokenBucket>,
        event_tx: broadcast::Sender<DomainEvent>,
        max_concurrent: usize,
        max_task_wait: Duration,
        cache_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            pool,
            chunk_repo,
            client,
            cache,
            rate_limiter,
            event_tx,
            notify: Arc::new(Notify::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            max_task_wait,
            cache_dir,
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
            let client = self.client.clone();
            let cache = self.cache.clone();
            let rate_limiter = self.rate_limiter.clone();
            let event_tx = self.event_tx.clone();
            let notify = self.notify.clone();
            let semaphore = self.semaphore.clone();
            let cancel = self.cancelled.clone();
            let cache_dir = self.cache_dir.clone();

            tokio::spawn(async move {
                worker_loop(
                    i,
                    chunk_repo,
                    client,
                    cache,
                    rate_limiter,
                    event_tx,
                    notify,
                    semaphore,
                    cancel,
                    cache_dir,
                )
                .await;
            });
        }
    }

    /// Signal all workers to shut down gracefully.
    pub fn shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);
        for _ in 0..self.max_concurrent {
            self.notify.notify_one();
        }
    }
}

async fn worker_loop(
    worker_id: usize,
    chunk_repo: Arc<dyn ChunkRepo>,
    client: Arc<MimoClient>,
    cache: Arc<Cache>,
    rate_limiter: Arc<TokenBucket>,
    event_tx: broadcast::Sender<DomainEvent>,
    notify: Arc<Notify>,
    semaphore: Arc<Semaphore>,
    cancelled: Arc<AtomicBool>,
    cache_dir: std::path::PathBuf,
) {
    info!("ChunkQueue worker {worker_id} started");

    loop {
        if cancelled.load(Ordering::Acquire) {
            info!("ChunkQueue worker {worker_id} shutting down");
            return;
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
                error!("worker {worker_id}: find_pending_prioritized failed: {e}");
                drop(_permit);
                notify.notified().await;
                continue;
            }
        };

        // Mark chunk as processing
        if let Err(e) = chunk_repo.update_status(&chunk.id.to_string(), &ChunkStatus::Processing) {
            error!("worker {worker_id}: failed to mark chunk processing: {e}");
            drop(_permit);
            continue;
        }

        // Process the chunk
        let repo_c = chunk_repo.clone();
        let client_c = client.clone();
        let cache_c = cache.clone();
        let tx_c = event_tx.clone();
        let dir_c = cache_dir.clone();
        let chunk_id = chunk.id.to_string();
        let task_id = chunk.task_id.to_string();
        let seq = chunk.seq;
        let text = chunk.text.clone();

        tokio::spawn(async move {
            let result = process_chunk(
                repo_c.as_ref(),
                &client_c,
                cache_c.as_ref(),
                &tx_c,
                &chunk_id,
                &task_id,
                seq,
                &text,
                &dir_c,
            )
            .await;

            if let Err(e) = result {
                error!("chunk {chunk_id} processing failed: {e}");
            }
        });
    }
}

async fn process_chunk(
    chunk_repo: &dyn ChunkRepo,
    client: &MimoClient,
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

    // 2. Call MIMO API
    let audio_bytes = client.synthesize(text, "default", "tts-1", 1.0).await?;

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
    use std::path::PathBuf;
    use std::time::Duration;

    fn setup_test_queue() -> (DbPool, Arc<dyn ChunkRepo>, ChunkQueue) {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
        let client = Arc::new(MimoClient::new("test-key", "http://localhost:30231"));
        let cache = Arc::new(Cache::new(
            PathBuf::from("data/test_cache"),
            Duration::from_secs(300),
            100,
        ));
        let rate_limiter = Arc::new(TokenBucket::new(1000));
        let (event_tx, _) = broadcast::channel(512);
        let cache_dir = PathBuf::from("data/test_cache");

        let queue = ChunkQueue::new(
            pool.clone(),
            chunk_repo.clone(),
            client,
            cache,
            rate_limiter,
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
