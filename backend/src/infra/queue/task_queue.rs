//! Task-level orchestration queue.
//!
//! Manages the end-to-end lifecycle of a TTS task:
//!
//! 1. **enqueue** — chunk the source text, insert `Chunk` rows into the DB,
//!    then enqueue each chunk with the `ChunkQueue`.
//! 2. **listen** — react to `DomainEvent` messages from the chunk workers,
//!    track per-task progress, and trigger the final audio merge when all
//!    chunks complete.
//! 3. **continue_task** — re-enqueue chunks that were left incomplete after
//!    a restart or cache miss.
//! 4. **retry_merge** — re-attempt a failed audio merge.
//! 5. **merge_task_audio** — concatenate chunk WAV files into a single output.

#![allow(dead_code)]

use crate::domain::chunk::{Chunk, ChunkStatus};
use crate::domain::events::DomainEvent;
use crate::shared::id::Id;
use crate::domain::task::{Task, TaskStatus};
use crate::infra::audio::merger::merge_wavs;
use crate::infra::mimo::chunker::MimoChunker;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::persistence::db::DbPool;
use crate::infra::persistence::group_repo::GroupRepo;
use chrono;
use rusqlite;
use crate::infra::persistence::task_repo::TaskRepo;
use crate::infra::queue::chunk_queue::ChunkQueue;
use crate::shared::error::AppError;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Orchestrates a single TTS task from chunking through merging.
pub struct TaskQueue {
    pool: DbPool,
    task_repo: Arc<dyn TaskRepo>,
    chunk_repo: Arc<dyn ChunkRepo>,
    chunk_queue: Arc<ChunkQueue>,
    group_repo: Arc<dyn GroupRepo>,
    /// Event bus sender for domain events.
    /// This is a clone — the original is held by the service layer.
    event_tx: broadcast::Sender<DomainEvent>,
    chunker: MimoChunker,
}

impl TaskQueue {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: DbPool,
        task_repo: Arc<dyn TaskRepo>,
        chunk_repo: Arc<dyn ChunkRepo>,
        chunk_queue: Arc<ChunkQueue>,
        group_repo: Arc<dyn GroupRepo>,
        event_tx: broadcast::Sender<DomainEvent>,
        chunker: MimoChunker,
    ) -> Self {
        Self {
            pool,
            task_repo,
            chunk_repo,
            chunk_queue,
            group_repo,
            event_tx,
            chunker,
        }
    }

    /// Enqueue a task for processing.
    ///
    /// 1. Loads the task and verifies it is `Pending`.
    /// 2. Transitions the task: `Pending → Queued → Chunking`.
    /// 3. Tokenizes the text and splits into chunks via `MimoChunker`.
    /// 4. Inserts all chunks into the DB.
    /// 5. Updates chunk progress on the task row.
    /// 6. Transitions the task to `Processing`.
    /// 7. Enqueues each chunk with the `ChunkQueue`.
    ///
    /// On failure the task status is reset to `Pending` so the caller can retry.
    ///
    /// The task must already exist in the DB (created via the service layer).
    pub async fn enqueue(&self, task_id: &str) -> Result<(), AppError> {
        let mut task = self
            .task_repo
            .find_by_id(task_id)?
            .ok_or_else(|| AppError::NotFound(format!("Task {task_id}")))?;

        if task.status != TaskStatus::Pending {
            return Err(AppError::InvalidInput(format!(
                "Task {task_id} status is {:?}, expected Pending",
                task.status
            )));
        }

        // Transition to Queued — task stays here until a ChunkQueue worker
        // picks up the first chunk and transitions it to Processing.
        self.task_repo.update_status(task_id, &TaskStatus::Queued)?;
        let _ = self.event_tx.send(DomainEvent::TaskStatusChanged {
            task_id: task.id.clone(),
            batch_id: task.batch_id.clone(),
            status: "queued".to_string(),
        });

        // Tokenize and split into chunks — on failure, reset status so caller can retry.
        let tokenize_result = self.chunker.tokenize(&task.content).await;
        let sentences = match tokenize_result {
            Ok(s) => s,
            Err(e) => {
                warn!("enqueue: tokenize failed for task {task_id}, resetting to Pending: {e}");
                let _ = self.task_repo.update_status(task_id, &TaskStatus::Pending);
                return Err(e);
            }
        };

        let total_tokens: i64 = sentences.iter().map(|s| s.token_count).sum();
        task.total_tokens = total_tokens;

        let segments = self
            .chunker
            .split(&task.content, None)
            .await
            .map_err(|e| {
                warn!("enqueue: split failed for task {task_id}, resetting to Pending: {e}");
                let _ = self.task_repo.update_status(task_id, &TaskStatus::Pending);
                e
            })?;

        let task_id_obj = Id::from_str(task_id)?;
        let chunks: Vec<Chunk> = segments
            .into_iter()
            .enumerate()
            .map(|(i, seg)| {
                Chunk::new(task_id_obj.clone(), (i + 1) as i32, seg.text)
            })
            .collect();

        let total = chunks.len() as i32;
        self.chunk_repo.insert_batch(&chunks)?;
        self.task_repo.update_chunk_progress(task_id, total, 0, 0, total_tokens)?;

        // NOTE: Do NOT set Processing here — chunks haven't started yet.
        // The ChunkQueue worker will transition the task to Processing
        // when the first chunk actually begins synthesis.

        // Fire event.
        let _ = self.event_tx.send(DomainEvent::TaskEnqueued {
            task_id: task.id.clone(),
            batch_id: task.batch_id.clone(),
        });

        // Enqueue each chunk.
        for chunk in &chunks {
            self.chunk_queue
                .enqueue(&chunk.id.to_string(), task_id);
        }

        Ok(())
    }

    /// Listen for chunk-level events and react.
    ///
    /// Spawn this as a background task after `run_workers()` has been called
    /// on the `ChunkQueue`.  It will process events until the sender is
    /// dropped (i.e. the queue is shut down).
    ///
    /// Includes a periodic DB poll fallback (every 10s) to catch tasks that
    /// may have been missed due to broadcast channel lag.
    pub async fn listen(&self, mut event_rx: broadcast::Receiver<DomainEvent>) {
        let mut poll_interval = tokio::time::interval(std::time::Duration::from_secs(10));
        poll_interval.tick().await; // first tick completes immediately

        loop {
            tokio::select! {
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Err(e) = self.handle_event(event).await {
                                error!("event handler error: {e}");
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("task-queue event listener: channel closed, exiting");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("task-queue event listener lagged by {n} events — triggering DB poll fallback");
                            if let Err(e) = self.poll_and_reconcile().await {
                                error!("DB poll fallback failed: {e}");
                            }
                        }
                    }
                }
                _ = poll_interval.tick() => {
                    // Periodic DB poll to catch any missed events
                    if let Err(e) = self.poll_and_reconcile().await {
                        error!("periodic DB poll failed: {e}");
                    }
                }
            }
        }
    }

    /// DB poll fallback: find tasks stuck in non-terminal states whose chunks are
    /// all resolved and trigger completion. Also reconciles batch statuses.
    async fn poll_and_reconcile(&self) -> Result<(), AppError> {
        let all_tasks = self.task_repo.find_all()?;

        // Cover Processing, Queued, Merging, MergingFailed — all states that can get stuck
        let active_tasks: Vec<_> = all_tasks
            .iter()
            .filter(|t| matches!(t.status,
                TaskStatus::Processing | TaskStatus::Queued |
                TaskStatus::Merging | TaskStatus::MergingFailed))
            .collect();

        let mut reconciled = 0;
        for task in &active_tasks {
            let task_id = task.id.to_string();

            // MergingFailed: merge already failed, just ensure TaskFailed is emitted
            if task.status == TaskStatus::MergingFailed {
                warn!("poll_and_reconcile: task {task_id} stuck in MergingFailed — emitting TaskFailed");
                reconciled += 1;
                let _ = self.event_tx.send(DomainEvent::TaskFailed {
                    task_id: task.id.clone(),
                    error: "Merge failed (detected by poll)".to_string(),
                });
                continue;
            }

            let (total, done, failed, _pending, _processing) = self.chunk_repo.count_by_task_aggregated(&task_id)?;

            if total > 0 && done + failed == total {
                warn!(
                    "poll_and_reconcile: task {task_id} has all chunks resolved ({done} done, {failed} failed) but status is {:?} — triggering completion",
                    task.status
                );
                reconciled += 1;

                if done > 0 {
                    // Re-emit AllChunksDone — will trigger merge + TaskCompleted
                    let _ = self.event_tx.send(DomainEvent::AllChunksDone {
                        task_id: task.id.clone(),
                        total_chunks: total as i32,
                    });
                } else {
                    // All failed
                    self.task_repo.update_status(&task_id, &TaskStatus::Failed)?;
                    let _ = self.event_tx.send(DomainEvent::TaskFailed {
                        task_id: task.id.clone(),
                        error: format!("All {total} chunks failed"),
                    });
                }
            }

            // Timeout protection: if task has been stuck for > 180s with no chunks
            // or with chunks that have active work but haven't progressed,
            // mark it as failed to prevent batch hanging forever.
            if task.status == TaskStatus::Queued && total == 0 {
                // Task in Queued state with 0 chunks — enqueue may have failed
                // Check age via created_at
                let age_secs = (chrono::Utc::now() - task.created_at).num_seconds();
                if age_secs > 180 {
                    warn!("poll_and_reconcile: task {task_id} stuck in Queued for {age_secs}s with 0 chunks — marking failed");
                    reconciled += 1;
                    self.task_repo.update_status(&task_id, &TaskStatus::Failed)?;
                    let _ = self.event_tx.send(DomainEvent::TaskFailed {
                        task_id: task.id.clone(),
                        error: format!("Stuck in Queued for {age_secs}s, no chunks created"),
                    });
                }
            }
        }

        if reconciled > 0 {
            info!("poll_and_reconcile: reconciled {reconciled} stuck tasks");
        }

        // Also reconcile batch statuses (queued → processing → completed)
        self.reconcile_batch_statuses(&all_tasks)?;

        Ok(())
    }

    /// Scan all batches and update their status based on child task states.
    /// - "queued" batch with any active/done task → "processing"
    /// - "processing" batch with all terminal tasks → "completed"/"failed"
    fn reconcile_batch_statuses(&self, all_tasks: &[Task]) -> Result<(), AppError> {
        // Group tasks by batch_id
        let mut batch_map: std::collections::HashMap<String, Vec<&Task>> = std::collections::HashMap::new();
        for task in all_tasks {
            if let Some(ref bid) = task.batch_id {
                batch_map.entry(bid.to_string()).or_default().push(task);
            }
        }

        for (batch_id, tasks) in &batch_map {
            let total = tasks.len() as i32;
            if total == 0 { continue; }

            let done = tasks.iter().filter(|t| t.status == TaskStatus::Done).count() as i32;
            let failed = tasks.iter().filter(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::MergingFailed)).count() as i32;
            let active = tasks.iter().filter(|t| matches!(t.status,
                TaskStatus::Processing | TaskStatus::Merging | TaskStatus::Queued | TaskStatus::Chunking)).count() as i32;
            let terminal = done + failed;

            // If all tasks are terminal → update batch to completed/failed
            if terminal >= total {
                let now = chrono::Utc::now().to_rfc3339();
                if let Ok(conn) = self.pool.get() {
                    if done > 0 {
                        let _ = conn.execute(
                            "UPDATE batches SET status = '\"completed\"', updated_at = ?1, completed_at = COALESCE(completed_at, ?1) WHERE id = ?2 AND status != '\"completed\"'",
                            rusqlite::params![now, batch_id],
                        );
                        info!("reconcile: batch {batch_id} → completed ({done}/{total} done)");
                    } else {
                        let _ = conn.execute(
                            "UPDATE batches SET status = '\"failed\"', updated_at = ?1, completed_at = COALESCE(completed_at, ?1) WHERE id = ?2 AND status != '\"failed\"'",
                            rusqlite::params![now, batch_id],
                        );
                        info!("reconcile: batch {batch_id} → failed (all {failed} failed)");
                    }
                }
            } else if active > 0 {
                // Some tasks are active → ensure batch is "processing"
                if let Ok(conn) = self.pool.get() {
                    let _ = conn.execute(
                        "UPDATE batches SET status = '\"processing\"', updated_at = ?1 WHERE id = ?2 AND status = '\"queued\"'",
                        rusqlite::params![chrono::Utc::now().to_rfc3339(), batch_id],
                    );
                }
            }
        }

        Ok(())
    }

    async fn handle_event(&self, event: DomainEvent) -> Result<(), AppError> {
        match &event {
            DomainEvent::ChunkCompleted {
                task_id,
                chunk_id: _,
                seq: _,
                audio_path: _,
                duration: _,
            } => {
                self.on_chunk_completed(task_id.as_str()).await?;
            }
            DomainEvent::ChunkFailed {
                task_id,
                chunk_id: _,
                seq: _,
                error: _,
                retry_count: _,
            } => {
                self.on_chunk_failed(task_id.as_str()).await?;
            }
            DomainEvent::AllChunksDone { task_id, .. } => {
                self.on_all_chunks_done(task_id.as_str()).await?;
            }
            DomainEvent::TaskCompleted { task_id, batch_id, .. } => {
                self.on_task_completed(task_id.as_str(), batch_id.as_ref()).await?;
            }
            DomainEvent::TaskFailed { task_id, .. } => {
                self.on_task_failed(task_id.as_str()).await?;
            }
            DomainEvent::TaskStatusChanged { task_id, status, .. } if status == "cancelled" => {
                // Cancelled tasks count as failed for group completion tracking
                let task = self.task_repo.find_by_id(task_id.as_str())?;
                if let Some(ref task) = task {
                    if let Some(ref gid) = task.group_id {
                        info!("Task {task_id} cancelled — updating group {gid} counter");
                        self.check_group_completion(gid.as_str(), false).await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_chunk_completed(&self, task_id: &str) -> Result<(), AppError> {
        let (total, done, failed, _pending, _processing) = self.chunk_repo.count_by_task_aggregated(task_id)?;

        self.task_repo
            .update_chunk_progress(task_id, total as i32, done as i32, failed as i32, 0)?;

        // All chunks resolved (done + failed == total) → trigger merge/complete
        if done + failed == total && total > 0 {
            if done > 0 {
                // At least some succeeded → merge what we have
                let _ = self.event_tx.send(DomainEvent::AllChunksDone {
                    task_id: Id::from_str(task_id)?,
                    total_chunks: total as i32,
                });
            } else {
                // All failed → mark task failed
                self.task_repo.update_status(task_id, &TaskStatus::Failed)?;
                let _ = self.event_tx.send(DomainEvent::TaskFailed {
                    task_id: Id::from_str(task_id)?,
                    error: format!("All {total} chunks failed"),
                });
            }
        }
        Ok(())
    }

    async fn on_chunk_failed(&self, task_id: &str) -> Result<(), AppError> {
        let (total, done, failed, _pending, _processing) = self.chunk_repo.count_by_task_aggregated(task_id)?;

        self.task_repo
            .update_chunk_progress(task_id, total as i32, done as i32, failed as i32, 0)?;

        // All chunks resolved (done + failed == total) → trigger merge/complete
        if done + failed == total && total > 0 {
            if done > 0 {
                // At least some succeeded → merge what we have
                let _ = self.event_tx.send(DomainEvent::AllChunksDone {
                    task_id: Id::from_str(task_id)?,
                    total_chunks: total as i32,
                });
            } else {
                // All failed → mark task failed
                self.task_repo.update_status(task_id, &TaskStatus::Failed)?;
                let _ = self.event_tx.send(DomainEvent::TaskFailed {
                    task_id: Id::from_str(task_id)?,
                    error: format!("All {total} chunks failed"),
                });
            }
        }
        Ok(())
    }

    async fn on_all_chunks_done(&self, task_id: &str) -> Result<(), AppError> {
        // Set status to Merging synchronously
        self.task_repo
            .update_status(task_id, &TaskStatus::Merging)?;

        // Spawn merge in spawn_blocking to avoid blocking the tokio runtime.
        // Streaming I/O (BufReader/BufWriter) keeps memory usage low even for large tasks.
        let task_id_owned = task_id.to_string();
        let task_repo = Arc::clone(&self.task_repo);
        let event_tx = self.event_tx.clone();
        let chunk_repo = Arc::clone(&self.chunk_repo);

        tokio::spawn(async move {
            let task_id_for_merge = task_id_owned.clone();
            let merge_result = tokio::task::spawn_blocking(move || {
                Self::merge_task_audio_static(&chunk_repo, &task_id_for_merge)
            }).await;

            let merger_result = match merge_result {
                Ok(inner) => inner,
                Err(join_err) => {
                    error!("merge spawn_blocking failed for task {task_id_owned}: {join_err}");
                    let _ = task_repo.update_status(&task_id_owned, &TaskStatus::MergingFailed);
                    return;
                }
            };

            match merger_result {
                Ok((output_path, duration)) => {
                    let _ = task_repo.set_output(
                        &task_id_owned,
                        &output_path.to_string_lossy(),
                        duration,
                    );
                    let _ = task_repo.update_status(&task_id_owned, &TaskStatus::Done);

                    if let Ok(Some(task)) = task_repo.find_by_id(&task_id_owned) {
                        let _ = event_tx.send(DomainEvent::TaskCompleted {
                            task_id: task.id,
                            batch_id: task.batch_id,
                            output_path: output_path.to_string_lossy().to_string(),
                            duration,
                        });
                    }
                }
                Err(e) => {
                    error!("merge failed for task {task_id_owned}: {e}");
                    let _ = task_repo.update_status(
                        &task_id_owned,
                        &TaskStatus::MergingFailed,
                    );
                    if let Ok(tid) = Id::from_str(&task_id_owned) {
                        let _ = event_tx.send(DomainEvent::TaskFailed {
                            task_id: tid,
                            error: e.to_string(),
                        });
                    }
                }
            }
        });

        Ok(())
    }

    async fn on_task_completed(&self, task_id: &str, batch_id: Option<&Id>) -> Result<(), AppError> {
        if let Some(bid) = batch_id {
            self.check_batch_completion(bid.as_str()).await?;
        }
        // Check group completion
        if let Some(task) = self.task_repo.find_by_id(task_id)? {
            if let Some(ref gid) = task.group_id {
                self.check_group_completion(gid.as_str(), true).await?;
            }
        }
        Ok(())
    }

    async fn on_task_failed(&self, task_id: &str) -> Result<(), AppError> {
        // Find the task to get its batch_id
        if let Some(task) = self.task_repo.find_by_id(task_id)? {
            if let Some(ref bid) = task.batch_id {
                self.check_batch_completion(bid.as_str()).await?;
            }
            // Check group completion
            if let Some(ref gid) = task.group_id {
                self.check_group_completion(gid.as_str(), false).await?;
            }
        }
        Ok(())
    }

    /// Check if all tasks in a batch are terminal (done or failed).
    /// If so, update batch status in DB and emit BatchCompleted or BatchFailed.
    async fn check_batch_completion(&self, batch_id: &str) -> Result<(), AppError> {
        let progress = self.task_repo.batch_progress(batch_id)?;
        let terminal = progress.done_tasks + progress.failed_tasks;

        if terminal >= progress.total_tasks && progress.total_tasks > 0 {
            let now = chrono::Utc::now().to_rfc3339();
            let conn = self.pool.get()?;
            if progress.done_tasks > 0 {
                conn.execute(
                    "UPDATE batches SET status = '\"completed\"', updated_at = ?1, completed_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, batch_id],
                )?;
                info!("Batch {batch_id} marked as completed in DB ({}/{} done)", progress.done_tasks, progress.total_tasks);
                let _ = self.event_tx.send(DomainEvent::BatchCompleted {
                    batch_id: Id::from_str(batch_id)?,
                });
            } else {
                conn.execute(
                    "UPDATE batches SET status = '\"failed\"', updated_at = ?1, completed_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, batch_id],
                )?;
                info!("Batch {batch_id} marked as failed in DB (all {} tasks failed)", progress.failed_tasks);
                let _ = self.event_tx.send(DomainEvent::BatchFailed {
                    batch_id: Id::from_str(batch_id)?,
                    error: format!("All {} tasks failed", progress.failed_tasks),
                    failed_count: progress.failed_tasks,
                });
            }
        }
        Ok(())
    }

    /// Check if all tasks in a group are terminal (done or failed).
    /// If so, update group status and emit GroupCompleted or GroupFailed.
    async fn check_group_completion(&self, group_id: &str, task_succeeded: bool) -> Result<(), AppError> {
        info!("check_group_completion called: group={group_id}, task_succeeded={task_succeeded}");
        let group = if task_succeeded {
            self.group_repo.increment_done_tasks(group_id)?
        } else {
            self.group_repo.increment_failed_tasks(group_id)?
        };

        // If group transitioned to terminal status (Completed/Failed), emit event
        match group.status {
            crate::domain::group::GroupStatus::Completed => {
                let _ = self.event_tx.send(DomainEvent::GroupCompleted {
                    group_id: group.id.clone(),
                    batch_id: group.batch_id.clone(),
                });
                info!("Group {group_id} completed: {}/{} done, {} failed",
                    group.done_tasks, group.total_tasks, group.failed_tasks);
            }
            crate::domain::group::GroupStatus::Failed => {
                let _ = self.event_tx.send(DomainEvent::GroupFailed {
                    group_id: group.id.clone(),
                    batch_id: group.batch_id.clone(),
                    error: format!("Group failed: {}/{} failed", group.failed_tasks, group.total_tasks),
                });
                info!("Group {group_id} failed: {}/{} done, {} failed",
                    group.done_tasks, group.total_tasks, group.failed_tasks);
            }
            _ => {
                // Group still in progress — just log progress
                info!("Group {group_id} progress: {}/{} done, {} failed",
                    group.done_tasks, group.total_tasks, group.failed_tasks);
            }
        }
        Ok(())
    }

    /// Force a Queued task into processing by waking all workers.
    ///
    /// Workers are sleeping on `notify.notified()`, rate limiter, or chunk semaphore.
    /// `wake_all()` wakes them all; the next polling cycle picks up any pending
    /// chunks belonging to this task.
    pub fn force_process(&self, task_id: &str) -> Result<(), AppError> {
        let task = self
            .task_repo
            .find_by_id(task_id)?
            .ok_or_else(|| AppError::NotFound(format!("Task {task_id}")))?;

        // Only Queued tasks are eligible — they already have pending chunks.
        if task.status != TaskStatus::Queued {
            return Err(AppError::InvalidInput(format!(
                "Task {task_id} status is {:?}, only Queued tasks can be force-processed",
                task.status
            )));
        }

        // Reset any non-Pending chunks back to Pending so workers pick them up.
        let chunks = self.chunk_repo.find_by_task(task_id)?;
        for chunk in &chunks {
            if chunk.status != ChunkStatus::Pending {
                let _ = self
                    .chunk_repo
                    .update_status(&chunk.id.to_string(), &ChunkStatus::Pending);
            }
        }

        // Wake all workers so they immediately scan for pending chunks.
        self.chunk_queue.wake_all();
        info!("force-process: woke all workers for task {task_id}");
        Ok(())
    }

    /// Re-enqueue chunks that are still `Pending`, `Failed`, or `Done`
    /// but missing audio (cache miss / incomplete restart).
    pub fn continue_task(&self, task_id: &str) -> Result<(), AppError> {
        let chunks = self.chunk_repo.find_by_task(task_id)?;

        for chunk in chunks {
            let needs_retry = match chunk.status {
                ChunkStatus::Failed | ChunkStatus::Pending => true,
                ChunkStatus::Done => chunk.audio_path.is_none(),
                _ => false,
            };

            if needs_retry {
                // Reset to Pending so the worker picks it up again.
                let _ = self
                    .chunk_repo
                    .update_status(&chunk.id.to_string(), &ChunkStatus::Pending);
                self.chunk_queue
                    .enqueue(&chunk.id.to_string(), task_id);
            }
        }

        Ok(())
    }

    /// Retry the audio merge for a task that previously failed.
    ///
    /// Returns an error if the task is not `MergingFailed` or if not all
    /// chunks have completed.
    pub fn retry_merge(&self, task_id: &str) -> Result<(), AppError> {
        let task = self
            .task_repo
            .find_by_id(task_id)?
            .ok_or_else(|| AppError::NotFound(format!("Task {task_id}")))?;

        if task.status != TaskStatus::MergingFailed && task.status != TaskStatus::Merging {
            return Err(AppError::InvalidInput(format!(
                "Task {task_id} status is {:?}, expected MergingFailed or Merging",
                task.status
            )));
        }

        let chunks = self.chunk_repo.find_by_task(task_id)?;
        if chunks.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Task {task_id} has no chunks"
            )));
        }

        // Check all chunks are Done.
        let all_done = chunks.iter().all(|c| c.status == ChunkStatus::Done);
        if !all_done {
            return Err(AppError::InvalidInput(format!(
                "Task {task_id} has incomplete chunks, cannot merge"
            )));
        }

        self.task_repo
            .update_status(task_id, &TaskStatus::Merging)?;

        match self.merge_task_audio(task_id) {
            Ok((output_path, duration)) => {
                self.task_repo
                    .set_output(task_id, &output_path.to_string_lossy(), duration)?;
                self.task_repo.update_status(task_id, &TaskStatus::Done)?;
                let task = self.task_repo.find_by_id(task_id)?.unwrap();
                let _ = self.event_tx.send(DomainEvent::TaskCompleted {
                    task_id: task.id,
                    batch_id: task.batch_id,
                    output_path: output_path.to_string_lossy().to_string(),
                    duration,
                });
                Ok(())
            }
            Err(e) => {
                self.task_repo
                    .update_status(task_id, &TaskStatus::MergingFailed)?;
                let _ = self.event_tx.send(DomainEvent::TaskFailed {
                    task_id: Id::from_str(task_id)?,
                    error: e.to_string(),
                });
                Err(e)
            }
        }
    }

    /// Concatenate all Done chunk WAV files for a task into a single output.
    ///
    /// Collects audio file paths from done chunks, creates a timestamped
    /// output file, and delegates to `merge_wavs` for the actual
    /// concatenation.
    fn merge_task_audio(&self, task_id: &str) -> Result<(PathBuf, f64), AppError> {
        Self::merge_task_audio_static(&self.chunk_repo, task_id)
    }

    /// Static version of merge_task_audio that can be called from a spawned task
    /// without holding a reference to `self`.
    fn merge_task_audio_static(
        chunk_repo: &Arc<dyn ChunkRepo>,
        task_id: &str,
    ) -> Result<(PathBuf, f64), AppError> {
        let mut chunks = chunk_repo.find_by_task(task_id)?;
        chunks.sort_by_key(|c| c.seq);

        if chunks.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Task {task_id} has no chunks"
            )));
        }

        let done_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.status == ChunkStatus::Done)
            .collect();

        if done_chunks.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "No Done chunks for task {task_id}"
            )));
        }

        let chunk_paths: Vec<PathBuf> = done_chunks
            .iter()
            .map(|c| {
                c.audio_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        // Fallback: try to find the file in a known location.
                        let cache_dir = std::env::temp_dir().join("chunks");
                        cache_dir.join(format!("{}.wav", c.id))
                    })
            })
            .collect();

        // Verify all files exist.
        for path in &chunk_paths {
            if !path.exists() {
                return Err(AppError::NotFound(format!(
                    "Missing chunk audio: {}",
                    path.display()
                )));
            }
        }

        let output_dir = std::path::Path::new("data").join("output");
        std::fs::create_dir_all(&output_dir).map_err(|e| {
            AppError::Internal(format!("Failed to create output dir: {e}"))
        })?;

        let output_filename = format!("task_{task_id}.wav");
        let output_path = output_dir.join(&output_filename);

        merge_wavs(&chunk_paths, &output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chunk::Chunk;
use crate::shared::id::Id;
    use crate::domain::task::{CreateTaskRequest, Task, TaskType};
    use crate::infra::mimo::chunker::MimoChunker;
    use crate::infra::persistence::chunk_repo::SqliteChunkRepo;
    use crate::infra::persistence::db::create_test_pool;
    use crate::infra::persistence::migrate::run_migrations;
    use crate::infra::persistence::task_repo::SqliteTaskRepo;
    use crate::infra::persistence::provider_repo::{ProviderRepo, SqliteProviderRepo};
    use crate::infra::cache::Cache;
    use crate::infra::queue::rate_limiter::TokenBucket;
    use crate::infra::queue::chunk_queue::ChunkQueue;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::broadcast;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper: set up the full queue stack with in-memory SQLite and wiremock.
    async fn setup(
    ) -> (DbPool, Arc<dyn TaskRepo>, Arc<dyn ChunkRepo>, Arc<ChunkQueue>, TaskQueue, MockServer) {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();

        let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
        let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));

        let mock_server = MockServer::start().await;
        let client = Arc::new(crate::infra::mimo::client::MimoClient::new(
            &mock_server.uri(),
        ));

        let cache_dir = std::env::temp_dir().join("task_queue_test").join(Id::new().to_string());
        let _ = std::fs::create_dir_all(&cache_dir);
        let cache = Arc::new(Cache::new(cache_dir.clone(), Duration::from_secs(300), 100));
        let rate_limiter = Arc::new(TokenBucket::new(1000));
        let token_budget = Arc::new(TokenBucket::new(1_000_000));
        let (event_tx, _event_rx) = broadcast::channel(512);
        let provider_rate_limiters = Arc::new(crate::infra::queue::rate_limiter::ProviderRateLimiterMap::new(1000, 10_000_000, 10));
        let load_balancer = Arc::new(crate::infra::queue::provider_balancer::ProviderLoadBalancer::new());

        let provider_repo: Arc<dyn ProviderRepo> =
            Arc::new(SqliteProviderRepo::new(pool.clone()));
        // Configure default provider for tests
        let _ = provider_repo.update_api_key("xiaomi", "test-key");

        let chunk_queue = Arc::new(ChunkQueue::new(
            pool.clone(),
            chunk_repo.clone(),
            task_repo.clone(),
            client,
            cache,
            rate_limiter,
            token_budget,
            event_tx.clone(),
            2,
            20,
            Duration::from_secs(300),
            cache_dir,
            provider_repo,
            provider_rate_limiters,
            load_balancer,
        ));

        let chunker = MimoChunker::new(&mock_server.uri(), 2000, 5000);
        let group_repo: Arc<dyn crate::infra::persistence::group_repo::GroupRepo> =
            Arc::new(crate::infra::persistence::group_repo::SqliteGroupRepo::new(pool.clone()));

        let task_queue = TaskQueue::new(
            pool.clone(),
            task_repo.clone(),
            chunk_repo.clone(),
            chunk_queue.clone(),
            group_repo,
            event_tx,
            chunker,
        );

        (pool, task_repo, chunk_repo, chunk_queue, task_queue, mock_server)
    }

    /// Create a minimal valid test WAV.
    fn test_wav() -> Vec<u8> {
        let sample_rate = 16000u32;
        let channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let duration_ms = 50u32;
        let bytes_per_sample = (bits_per_sample / 8) as u32;
        let data_size = sample_rate * channels as u32 * bytes_per_sample * duration_ms / 1000;
        let data_size = data_size + (data_size % 2);
        let file_size = 44u32 + data_size - 8;

        let mut wav = Vec::with_capacity((44 + data_size) as usize);
        wav.extend(b"RIFF");
        wav.extend(&file_size.to_le_bytes());
        wav.extend(b"WAVE");
        wav.extend(b"fmt ");
        wav.extend(&16u32.to_le_bytes());
        wav.extend(&1u16.to_le_bytes());
        wav.extend(&channels.to_le_bytes());
        wav.extend(&sample_rate.to_le_bytes());
        wav.extend(&(sample_rate * channels as u32 * bytes_per_sample).to_le_bytes());
        wav.extend(&(channels * bytes_per_sample as u16).to_le_bytes());
        wav.extend(&bits_per_sample.to_le_bytes());
        wav.extend(b"data");
        wav.extend(&data_size.to_le_bytes());
        wav.resize((44 + data_size) as usize, 0u8);
        wav
    }

    // -----------------------------------------------------------------
    // test_task_queue_enqueue_creates_chunks
    // -----------------------------------------------------------------
    #[actix_rt::test]
    async fn test_task_queue_enqueue_creates_chunks() {
        let (_pool, task_repo, chunk_repo, _chunk_queue, task_queue, mock_server) = setup().await;

        // Mock the tokenize endpoint (used by MimoChunker).
        Mock::given(method("POST"))
            .and(path("/v1/tokenize"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sentences": [
                    {"text": "Hello world. This is a test.", "token_count": 5, "char_count": 28}
                ]
            })))
            .mount(&mock_server)
            .await;

        let req = CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "Hello world. This is a test.".into(),
            content_ref: None,
            title: "Test task".into(),
            voice: crate::constants::DEFAULT_VOICE.into(),
            model: crate::constants::DEFAULT_MODEL.into(),
            style: None,
            speed: 1.0,
            provider_id: None,
            total_chars: 28,
            total_tokens: 5,
        };
        let task = Task::new(req);
        task_repo.insert(&task).unwrap();
        let task_id = task.id.to_string();

        task_queue.enqueue(&task_id).await.unwrap();

        let stored = task_repo.find_by_id(&task_id).unwrap().unwrap();
        // After enqueue, task should be in Queued status (not Processing).
        // Processing is set by ChunkQueue worker when first chunk starts.
        assert_eq!(stored.status, TaskStatus::Queued);

        let chunks = chunk_repo.find_by_task(&task_id).unwrap();
        assert!(!chunks.is_empty(), "should have created chunks");
        for chunk in &chunks {
            assert_eq!(chunk.task_id.to_string(), task_id);
        }
    }

    // -----------------------------------------------------------------
    // test_task_queue_continue_cache_miss
    // -----------------------------------------------------------------
    #[actix_rt::test]
    async fn test_task_queue_continue_cache_miss() {
        let (_pool, task_repo, chunk_repo, _chunk_queue, task_queue, _mock_server) = setup().await;

        // Create a parent task for FK compliance.
        let task_id = Id::new();
        let task_id_str = task_id.to_string();
        let req = CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "test".into(),
            content_ref: None,
            title: "continue".into(),
            voice: "default".into(),
            model: crate::constants::DEFAULT_MODEL.into(),
            style: None,
            speed: 1.0,
            provider_id: None,
            total_chars: 10,
            total_tokens: 5,
        };
        let task = Task::new(req);
        // Override the ID to match our chunks.
        let task = Task {
            id: task_id.clone(),
            ..task
        };
        task_repo.insert(&task).unwrap();

        // Insert chunks: one Done (no audio_path → simulated cache miss),
        // one Failed, one Pending.
        let mut chunk1 = Chunk::new(task_id.clone(), 1, "done but missing audio".into());
        chunk1.status = ChunkStatus::Done;
        chunk_repo.insert(&chunk1).unwrap();

        let mut chunk2 = Chunk::new(task_id.clone(), 2, "failed".into());
        chunk2.status = ChunkStatus::Failed;
        chunk_repo.insert(&chunk2).unwrap();

        let chunk3 = Chunk::new(task_id.clone(), 3, "pending".into());
        chunk_repo.insert(&chunk3).unwrap();

        // Run continue_task.
        task_queue.continue_task(&task_id_str).unwrap();

        // Check the Done chunk without audio is now Pending.
        let c1 = chunk_repo.find_by_id(&chunk1.id.to_string()).unwrap().unwrap();
        assert_eq!(c1.status, ChunkStatus::Pending);

        // Check Failed chunk is unchanged (it was reset to Pending).
        let c2 = chunk_repo.find_by_id(&chunk2.id.to_string()).unwrap().unwrap();
        assert_eq!(c2.status, ChunkStatus::Pending);

        // Check Pending chunk is still Pending.
        let c3 = chunk_repo.find_by_id(&chunk3.id.to_string()).unwrap().unwrap();
        assert_eq!(c3.status, ChunkStatus::Pending);
    }

    // -----------------------------------------------------------------
    // test_task_queue_retry_merge_success
    // -----------------------------------------------------------------
    #[actix_rt::test]
    async fn test_task_queue_retry_merge_success() {
        let (pool, task_repo, chunk_repo, _chunk_queue, task_queue, _mock_server) = setup().await;

        let req = CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "test".into(),
            content_ref: None,
            title: "Merge test".into(),
            voice: crate::constants::DEFAULT_VOICE.into(),
            model: crate::constants::DEFAULT_MODEL.into(),
            style: None,
            speed: 1.0,
            provider_id: None,
            total_chars: 4,
            total_tokens: 2,
        };
        let task = Task::new(req);

        // We need to manually set MergingFailed via update_status which does
        // a transition check. The transition from Pending to MergingFailed
        // is not valid. So create chunks, set them done, then set the task
        // to MergingFailed by directly manipulating the DB status behind the scenes.
        // Easier: just insert and set the status column directly via raw SQL.

        task_repo.insert(&task).unwrap();
        let task_id = task.id.to_string();

        // Create chunks marked as Done with real WAV files.
        let temp_dir = std::env::temp_dir().join("merge_test").join(&task_id);
        let _ = std::fs::create_dir_all(&temp_dir);

        let wav_data = test_wav();

        let mut chunk1 = Chunk::new(task.id.clone(), 1, "part1".into());
        let path1 = temp_dir.join("chunk1.wav");
        std::fs::write(&path1, &wav_data).unwrap();
        chunk1.status = ChunkStatus::Done;
        chunk1.audio_path = Some(path1.to_string_lossy().to_string());
        chunk_repo.insert(&chunk1).unwrap();

        let mut chunk2 = Chunk::new(task.id.clone(), 2, "part2".into());
        let path2 = temp_dir.join("chunk2.wav");
        std::fs::write(&path2, &wav_data).unwrap();
        chunk2.status = ChunkStatus::Done;
        chunk2.audio_path = Some(path2.to_string_lossy().to_string());
        chunk_repo.insert(&chunk2).unwrap();

        // Set task to MergingFailed via raw SQL to bypass transition checks.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE tasks SET status = ?1 WHERE id = ?2",
                rusqlite::params![
                    serde_json::to_string(&TaskStatus::MergingFailed).unwrap(),
                    task_id
                ],
            )
            .unwrap();
        }

        // Retry merge.
        let result = task_queue.retry_merge(&task_id);
        assert!(result.is_ok(), "merge should succeed: {:?}", result.err());

        // Verify task is Done.
        let updated = task_repo.find_by_id(&task_id).unwrap().unwrap();
        assert_eq!(updated.status, TaskStatus::Done);
        assert!(updated.output_path.is_some());
    }

    // -----------------------------------------------------------------
    // test_task_queue_retry_merge_fails_when_not_done
    // -----------------------------------------------------------------
    #[actix_rt::test]
    async fn test_task_queue_retry_merge_fails_when_not_done() {
        let (pool, task_repo, chunk_repo, _chunk_queue, task_queue, _mock_server) = setup().await;

        let req = CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "test".into(),
            content_ref: None,
            title: "Merge fail test".into(),
            voice: crate::constants::DEFAULT_VOICE.into(),
            model: crate::constants::DEFAULT_MODEL.into(),
            style: None,
            speed: 1.0,
            provider_id: None,
            total_chars: 4,
            total_tokens: 2,
        };
        let task = Task::new(req);
        task_repo.insert(&task).unwrap();
        let task_id = task.id.to_string();

        // Create one Done chunk and one Pending chunk.
        let temp_dir = std::env::temp_dir().join("merge_fail_test").join(&task_id);
        let _ = std::fs::create_dir_all(&temp_dir);

        let wav_data = test_wav();
        let mut chunk1 = Chunk::new(task.id.clone(), 1, "part1".into());
        let path1 = temp_dir.join("chunk1.wav");
        std::fs::write(&path1, &wav_data).unwrap();
        chunk1.status = ChunkStatus::Done;
        chunk1.audio_path = Some(path1.to_string_lossy().to_string());
        chunk_repo.insert(&chunk1).unwrap();

        let chunk2 = Chunk::new(task.id.clone(), 2, "part2".into());
        chunk_repo.insert(&chunk2).unwrap(); // still Pending

        // Set task to MergingFailed via raw SQL.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE tasks SET status = ?1 WHERE id = ?2",
                rusqlite::params![
                    serde_json::to_string(&TaskStatus::MergingFailed).unwrap(),
                    task_id
                ],
            )
            .unwrap();
        }

        // Merge should fail because not all chunks are Done.
        let err = task_queue.retry_merge(&task_id).unwrap_err();
        assert!(
            err.to_string().contains("incomplete chunks"),
            "expected 'incomplete chunks' error, got: {err}"
        );
    }
}
