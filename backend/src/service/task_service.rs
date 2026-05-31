//! Task service — thin wrapper around TaskRepo + TaskQueue.
//!
//! Provides high-level operations for single TTS tasks:
//! - `create_single` — create a new task and persist it
//! - `get` — lookup by id
//! - `enqueue` — submit to the TaskQueue (async)
//! - `continue_task` — recover an incomplete task after restart

#![allow(dead_code)]

use crate::domain::events::DomainEvent;
use crate::domain::task::{CreateTaskRequest, Task, TaskStatus, TaskType};
use crate::infra::persistence::task_repo::TaskRepo;
use crate::infra::persistence::chunk_repo::ChunkRepo;
use crate::infra::queue::task_queue::TaskQueue;
use crate::shared::error::AppError;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Stateless service that wraps TaskRepo persistence and TaskQueue orchestration.
pub struct TaskService {
    pub task_repo: Arc<dyn TaskRepo>,
    pub chunk_repo: Arc<dyn ChunkRepo>,
    task_queue: Arc<TaskQueue>,
    event_tx: broadcast::Sender<DomainEvent>,
}

impl TaskService {
    pub fn new(
        task_repo: Arc<dyn TaskRepo>,
        chunk_repo: Arc<dyn ChunkRepo>,
        task_queue: Arc<TaskQueue>,
        event_tx: broadcast::Sender<DomainEvent>,
    ) -> Self {
        Self {
            task_repo,
            chunk_repo,
            task_queue,
            event_tx,
        }
    }

    /// Cancel a single task — sets status to Cancelled, cancels all pending/processing chunks.
    pub fn cancel(&self, task_id: &str) -> Result<(), AppError> {
        let task = self
            .task_repo
            .find_by_id(task_id)?
            .ok_or_else(|| AppError::NotFound(format!("Task {task_id}")))?;

        // Cancelled is reachable from: Pending, Queued, Chunking, Processing, Merging, MergingFailed, Paused, Failed
        let cancellable = matches!(
            task.status,
            TaskStatus::Pending
                | TaskStatus::Queued
                | TaskStatus::Chunking
                | TaskStatus::Processing
                | TaskStatus::Merging
                | TaskStatus::MergingFailed
                | TaskStatus::Paused
                | TaskStatus::Failed
        );

        if !cancellable {
            return Err(AppError::InvalidInput(format!(
                "Task status {:?} is not cancellable",
                task.status
            )));
        }

        // Set task status to Cancelled
        self.task_repo.update_status(task_id, &TaskStatus::Cancelled)?;

        // Cancel all pending/processing chunks
        let _ = self.chunk_repo.cancel_pending_by_task(task_id)?;

        // Emit TaskStatusChanged event
        let _ = self.event_tx.send(DomainEvent::TaskStatusChanged {
            task_id: task.id.clone(),
            batch_id: task.batch_id.clone(),
            status: "cancelled".to_string(),
        });

        Ok(())
    }

    /// Create a brand-new single (non-batch) task with `Pending` status.
    pub fn create_single(
        &self,
        content: String,
        title: String,
        voice: String,
        model: String,
        style: Option<String>,
        speed: f64,
    ) -> Result<Task, AppError> {
        let task = Task::new(CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content,
            content_ref: None,
            title,
            voice,
            model,
            style,
            speed,
            total_chars: 0,
            total_tokens: 0,
        });
        self.task_repo.insert(&task)?;
        Ok(task)
    }

    /// Fetch a task by its UUID.
    pub fn get(&self, id: &str) -> Result<Option<Task>, AppError> {
        self.task_repo.find_by_id(id)
    }

    /// Enqueue an existing task via the TaskQueue.
    ///
    /// The TaskQueue will chunk the content, insert chunk rows, and dispatch
    /// them to the ChunkQueue.
    pub async fn enqueue(&self, task_id: &str) -> Result<(), AppError> {
        self.task_queue.enqueue(task_id).await
    }

    /// Fetch tasks by batch ID.
    pub fn get_by_batch(&self, batch_id: &str) -> Result<Vec<Task>, AppError> {
        self.task_repo.find_by_batch(batch_id)
    }

    /// Recover an incomplete task after a restart (cache miss / crash).
    ///
    /// Delegates to `TaskQueue::continue_task`.
    pub async fn continue_task(&self, task_id: &str) -> Result<(), AppError> {
        self.task_queue.continue_task(task_id)
    }

    /// Retry a failed or stuck task: reset status to Pending, reset chunk progress, and re-enqueue.
    ///
    /// Accepts tasks in these states:
    /// - `Failed` / `MergingFailed` — normal retry after failure
    /// - `Queued` / `Chunking` / `Processing` — stuck tasks where chunks have all failed
    pub async fn retry(&self, task_id: &str) -> Result<(), AppError> {
        let task = self
            .task_repo
            .find_by_id(task_id)?
            .ok_or_else(|| AppError::NotFound(format!("Task {task_id}")))?;

        // Allow retry from any non-active state
        let retryable = matches!(
            task.status,
            crate::domain::task::TaskStatus::Failed
                | crate::domain::task::TaskStatus::MergingFailed
                | crate::domain::task::TaskStatus::Queued
                | crate::domain::task::TaskStatus::Chunking
                | crate::domain::task::TaskStatus::Processing
                | crate::domain::task::TaskStatus::Paused
                | crate::domain::task::TaskStatus::Cancelled
        );

        if !retryable {
            return Err(AppError::InvalidInput(format!(
                "Task status {:?} is not retryable",
                task.status
            )));
        }

        // Reset to Pending so enqueue() accepts it
        self.task_repo.update_status(task_id, &crate::domain::task::TaskStatus::Pending)?;
        // Reset chunk progress
        self.task_repo.update_chunk_progress(task_id, 0, 0, 0)?;
        // Delete old chunks so re-enqueue can create fresh ones
        let _ = self.chunk_repo.delete_by_task(task_id)?;

        self.task_queue.enqueue(task_id).await
    }

    /// Delete a task by ID.
    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        self.task_repo.delete(id)
    }

    /// Update task title.
    pub fn update_title(&self, id: &str, title: &str) -> Result<(), AppError> {
        self.task_repo.update_title(id, title)
    }
}
