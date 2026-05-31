//! Task Watchdog — patrols for stuck tasks.
//!
//! Periodically scans for tasks stuck in `Processing` status that have no
//! active (pending/processing) chunks in the queue. These tasks are marked
//! as `Failed` with an appropriate error message, and a `TaskFailed` event
//! is emitted so the frontend can update in real-time.
//!
//! This handles scenarios like:
//! - All chunks failed but task status was never updated
//! - Server crash during chunk processing
//! - Queue workers stopped unexpectedly

use crate::domain::chunk::ChunkStatus;
use crate::domain::events::DomainEvent;
use crate::domain::task::TaskStatus;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::persistence::task_repo::TaskRepo;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Configuration for the task watchdog.
pub struct WatchdogConfig {
    /// How often to run the patrol (default: 30 seconds).
    pub patrol_interval: Duration,
    /// How long a task can be in Processing before considered stale (default: 5 minutes).
    pub stale_threshold: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            patrol_interval: Duration::from_secs(15),
            stale_threshold: Duration::from_secs(60), // 1 minute
        }
    }
}

/// Task watchdog that patrols for stuck tasks.
pub struct TaskWatchdog {
    task_repo: Arc<dyn TaskRepo>,
    chunk_repo: Arc<dyn ChunkRepo>,
    event_tx: broadcast::Sender<DomainEvent>,
    config: WatchdogConfig,
}

impl TaskWatchdog {
    pub fn new(
        task_repo: Arc<dyn TaskRepo>,
        chunk_repo: Arc<dyn ChunkRepo>,
        event_tx: broadcast::Sender<DomainEvent>,
        config: WatchdogConfig,
    ) -> Self {
        Self {
            task_repo,
            chunk_repo,
            event_tx,
            config,
        }
    }

    /// Start the watchdog patrol loop.
    /// Returns a JoinHandle that can be used to stop the watchdog.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        let interval = self.config.patrol_interval;
        info!(
            "TaskWatchdog started — patrol every {:?}, stale threshold {:?}",
            interval, self.config.stale_threshold
        );

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            timer.tick().await; // first tick completes immediately

            loop {
                timer.tick().await;
                self.patrol().await;
            }
        })
    }

    /// Run a single patrol cycle.
    async fn patrol(&self) {
        let stale_minutes = self.config.stale_threshold.as_secs() as i64 / 60;
        let stale_tasks = match self.task_repo.find_stale_processing(stale_minutes) {
            Ok(tasks) => tasks,
            Err(e) => {
                error!("Watchdog: failed to query stale tasks: {e}");
                return;
            }
        };

        if stale_tasks.is_empty() {
            return; // nothing to do
        }

        warn!("Watchdog: found {} stale processing tasks", stale_tasks.len());

        for task in stale_tasks {
            let task_id = task.id.to_string();

            // Double-check: does this task still have active chunks?
            let pending = self
                .chunk_repo
                .count_by_task_status(&task_id, &ChunkStatus::Pending)
                .unwrap_or(0);
            let processing = self
                .chunk_repo
                .count_by_task_status(&task_id, &ChunkStatus::Processing)
                .unwrap_or(0);

            if pending > 0 || processing > 0 {
                info!(
                    "Watchdog: task {task_id} still has {pending} pending + {processing} processing chunks, skipping"
                );
                continue;
            }

            // No active chunks — this task is truly stuck
            warn!("Watchdog: marking stale task {task_id} as Failed");

            // Count chunk stats for error message
            let done = self
                .chunk_repo
                .count_by_task_status(&task_id, &ChunkStatus::Done)
                .unwrap_or(0);
            let failed = self
                .chunk_repo
                .count_by_task_status(&task_id, &ChunkStatus::Failed)
                .unwrap_or(0);

            let error_msg = format!(
                "Task stuck in processing — watchdog detected no active chunks ({done} done, {failed} failed)"
            );

            // Mark task as failed
            if let Err(e) = self.task_repo.update_status(&task_id, &TaskStatus::Failed) {
                error!("Watchdog: failed to mark task {task_id} as Failed: {e}");
                continue;
            }

            // Emit TaskFailed event for frontend
            let event = DomainEvent::TaskFailed {
                task_id: task.id.clone(),
                error: error_msg.clone(),
            };
            if let Err(e) = self.event_tx.send(event) {
                error!("Watchdog: failed to emit TaskFailed for {task_id}: {e}");
            }

            info!("Watchdog: task {task_id} marked as Failed — {error_msg}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chunk::Chunk;
    use crate::domain::task::{CreateTaskRequest, Task, TaskType};
    use crate::infra::persistence::chunk_repo::SqliteChunkRepo;
    use crate::infra::persistence::db::create_test_pool;
    use crate::infra::persistence::migrate::run_migrations;
    use crate::infra::persistence::task_repo::SqliteTaskRepo;
    use chrono::Utc;
    use rusqlite::params;

    fn setup() -> (SqliteTaskRepo, SqliteChunkRepo, broadcast::Sender<DomainEvent>) {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let task_repo = SqliteTaskRepo::new(pool.clone());
        let chunk_repo = SqliteChunkRepo::new(pool);
        let (event_tx, _) = broadcast::channel(100);
        (task_repo, chunk_repo, event_tx)
    }

    fn create_test_task(task_repo: &SqliteTaskRepo, status: TaskStatus) -> Task {
        let mut task = Task::new(CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "test content".into(),
            content_ref: None,
            title: "Test Task".into(),
            voice: "voice_1".into(),
            model: "model_1".into(),
            style: None,
            speed: 1.0,
            total_chars: 100,
            total_tokens: 50,
        });
        task.status = status;
        task_repo.insert(&task).unwrap();
        task
    }

    #[tokio::test]
    async fn test_watchdog_marks_stale_task_as_failed() {
        let (task_repo, chunk_repo, event_tx) = setup();

        // Create a task in Processing status with no chunks
        let task = create_test_task(&task_repo, TaskStatus::Processing);

        // Manually set updated_at to 10 minutes ago
        let old_time = (Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        task_repo.pool.get().unwrap()
            .execute(
                "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
                params![old_time, task.id.to_string()],
            )
            .unwrap();

        // Create watchdog with fresh repos pointing to same pool
        let task_repo2 = SqliteTaskRepo::new(task_repo.pool.clone());
        let watchdog = TaskWatchdog::new(
            Arc::new(task_repo2),
            Arc::new(chunk_repo),
            event_tx.clone(),
            WatchdogConfig {
                patrol_interval: Duration::from_secs(1),
                stale_threshold: Duration::from_secs(60), // 1 minute
            },
        );

        // Subscribe to events before patrol
        let mut rx = event_tx.subscribe();

        // Run patrol
        watchdog.patrol().await;

        // Task should be Failed
        let updated = task_repo.find_by_id(&task.id.to_string()).unwrap().unwrap();
        assert_eq!(updated.status, TaskStatus::Failed);

        // Should have emitted TaskFailed event
        let event = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(event.is_ok());
        match event.unwrap().unwrap() {
            DomainEvent::TaskFailed { task_id, error } => {
                assert_eq!(task_id, task.id);
                assert!(error.contains("watchdog"));
            }
            _ => panic!("Expected TaskFailed event"),
        }
    }

    #[tokio::test]
    async fn test_watchdog_skips_task_with_active_chunks() {
        let (task_repo, chunk_repo, event_tx) = setup();

        // Create a task in Processing status with active chunks
        let task = create_test_task(&task_repo, TaskStatus::Processing);

        // Add a pending chunk
        let chunk = Chunk::new(task.id.clone(), 1, "test text".into());
        chunk_repo.insert(&chunk).unwrap();

        // Set old updated_at
        let old_time = (Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        task_repo.pool.get().unwrap()
            .execute(
                "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
                params![old_time, task.id.to_string()],
            )
            .unwrap();

        let task_repo2 = SqliteTaskRepo::new(task_repo.pool.clone());
        let watchdog = TaskWatchdog::new(
            Arc::new(task_repo2),
            Arc::new(chunk_repo),
            event_tx.clone(),
            WatchdogConfig {
                patrol_interval: Duration::from_secs(1),
                stale_threshold: Duration::from_secs(60),
            },
        );

        // Run patrol
        watchdog.patrol().await;

        // Task should still be Processing (not failed)
        let updated = task_repo.find_by_id(&task.id.to_string()).unwrap().unwrap();
        assert_eq!(updated.status, TaskStatus::Processing);
    }
}
