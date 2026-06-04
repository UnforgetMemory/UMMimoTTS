use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::domain::events::DomainEvent;
use crate::domain::task::TaskStatus;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::persistence::task_repo::TaskRepo;
use crate::infra::queue::task_queue::TaskQueue;
use crate::shared::error::AppError;

/// Configuration for the QueuePatrol background service.
pub struct QueuePatrolConfig {
    /// How often the patrol runs (default: 30 seconds).
    pub patrol_interval: Duration,
    /// How long a task can stay in Pending before re-enqueue (default: 60 seconds).
    pub stale_pending_threshold: Duration,
    /// How long a task can stay in Queued with no chunks before re-enqueue (default: 120 seconds).
    pub stale_queued_threshold: Duration,
}

impl Default for QueuePatrolConfig {
    fn default() -> Self {
        Self {
            patrol_interval: Duration::from_secs(30),
            stale_pending_threshold: Duration::from_secs(60),
            stale_queued_threshold: Duration::from_secs(120),
        }
    }
}

/// Smart background service that patrols for orphaned tasks in the queue chain.
///
/// Detects and recovers:
/// 1. Tasks stuck in Pending state (never enqueued or failed during enqueue)
/// 2. Tasks stuck in Queued/Chunking state with no chunks (enqueue failed partway)
/// 3. Tasks in Processing where all chunks are done (missing AllChunksDone event)
pub struct QueuePatrol {
    config: QueuePatrolConfig,
    task_repo: Arc<dyn TaskRepo>,
    #[allow(dead_code)]
    chunk_repo: Arc<dyn ChunkRepo>,
    task_queue: Arc<TaskQueue>,
    event_tx: broadcast::Sender<DomainEvent>,
}

impl QueuePatrol {
    pub fn new(
        config: QueuePatrolConfig,
        task_repo: Arc<dyn TaskRepo>,
        chunk_repo: Arc<dyn ChunkRepo>,
        task_queue: Arc<TaskQueue>,
        event_tx: broadcast::Sender<DomainEvent>,
    ) -> Self {
        Self {
            config,
            task_repo,
            chunk_repo,
            task_queue,
            event_tx,
        }
    }

    /// Start the patrol loop in the background.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("QueuePatrol: starting (interval={:?})", self.config.patrol_interval);
            loop {
                tokio::time::sleep(self.config.patrol_interval).await;
                self.patrol_cycle().await;
            }
        })
    }

    /// Single patrol cycle — runs all checks sequentially.
    async fn patrol_cycle(&self) {
        self.recover_stale_pending().await;
        self.recover_stale_queued().await;
        self.recover_processing_all_done().await;
    }

    /// Phase 1: Find tasks stuck in Pending state.
    ///
    /// These tasks were either never enqueued, or the enqueue process failed and
    /// reset them to Pending but nobody picked them up. Re-enqueue them.
    async fn recover_stale_pending(&self) {
        let stale_minutes = self.config.stale_pending_threshold.as_secs() / 60;
        let stale_minutes = if stale_minutes == 0 { 1 } else { stale_minutes };

        let tasks = match self.task_repo.find_stale_pending(stale_minutes as i64) {
            Ok(t) => t,
            Err(e) => {
                error!("QueuePatrol: failed to query stale Pending tasks: {e}");
                return;
            }
        };

        if tasks.is_empty() {
            return;
        }

        info!("QueuePatrol: found {} stale Pending tasks, attempting re-enqueue", tasks.len());

        for task in &tasks {
            let task_id = task.id.to_string();
            info!("QueuePatrol: re-enqueuing stale Pending task {task_id}");
            match self.task_queue.enqueue(&task_id).await {
                Ok(()) => {
                    info!("QueuePatrol: successfully re-enqueued task {task_id}");
                }
                Err(AppError::InvalidInput(msg)) => {
                    // Task status is no longer Pending — another path recovered it
                    warn!("QueuePatrol: task {task_id} already recovered: {msg}");
                }
                Err(e) => {
                    error!("QueuePatrol: failed to re-enqueue task {task_id}: {e}");
                }
            }
        }
    }

    /// Phase 2: Find tasks stuck in Queued/Chunking with no chunks.
    ///
    /// These tasks had enqueue called but the tokenize/split/insert failed
    /// partway, leaving the task in Queued/Chunking with zero chunks.
    /// Reset them to Pending so they can be re-enqueued.
    async fn recover_stale_queued(&self) {
        let stale_minutes = self.config.stale_queued_threshold.as_secs() / 60;
        let stale_minutes = if stale_minutes == 0 { 1 } else { stale_minutes };

        let tasks = match self.task_repo.find_stuck_queued(stale_minutes as i64) {
            Ok(t) => t,
            Err(e) => {
                error!("QueuePatrol: failed to query stuck Queued tasks: {e}");
                return;
            }
        };

        if tasks.is_empty() {
            return;
        }

        info!("QueuePatrol: found {} stuck Queued tasks with no chunks, resetting to Pending", tasks.len());

        for task in &tasks {
            let task_id = task.id.to_string();
            info!("QueuePatrol: resetting stuck Queued task {task_id} to Pending");
            if let Err(e) = self.task_repo.update_status(&task_id, &TaskStatus::Pending) {
                error!("QueuePatrol: failed to reset task {task_id} to Pending: {e}");
                continue;
            }
            // Emit status change event
            let _ = self.event_tx.send(DomainEvent::TaskStatusChanged {
                task_id: task.id.clone(),
                batch_id: task.batch_id.clone(),
                status: "pending".to_string(),
            });
            // Now re-enqueue
            match self.task_queue.enqueue(&task_id).await {
                Ok(()) => {
                    info!("QueuePatrol: successfully re-enqueued stuck task {task_id}");
                }
                Err(e) => {
                    error!("QueuePatrol: failed to re-enqueue stuck task {task_id}: {e}");
                }
            }
        }
    }

    /// Phase 3: Find tasks in Processing where all chunks are done.
    ///
    /// This catches the edge case where the AllChunksDone event was lost
    /// (e.g., broadcast lag). The task has all chunks completed but nobody
    /// triggered the merge. Re-emit AllChunksDone.
    async fn recover_processing_all_done(&self) {
        let tasks = match self.task_repo.find_processing_all_done() {
            Ok(t) => t,
            Err(e) => {
                error!("QueuePatrol: failed to query Processing tasks with all chunks done: {e}");
                return;
            }
        };

        if tasks.is_empty() {
            return;
        }

        info!("QueuePatrol: found {} Processing tasks with all chunks done, re-emitting AllChunksDone", tasks.len());

        for task in &tasks {
            let task_id = task.id.to_string();
            info!("QueuePatrol: re-emitting AllChunksDone for task {task_id}");
            let _ = self.event_tx.send(DomainEvent::AllChunksDone {
                task_id: task.id.clone(),
                total_chunks: task.total_chunks,
            });
        }
    }
}
