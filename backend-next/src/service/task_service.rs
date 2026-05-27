//! Task service — thin wrapper around TaskRepo + TaskQueue.
//!
//! Provides high-level operations for single TTS tasks:
//! - `create_single` — create a new task and persist it
//! - `get` — lookup by id
//! - `enqueue` — submit to the TaskQueue (async)
//! - `continue_task` — recover an incomplete task after restart

#![allow(dead_code)]

use crate::domain::task::{CreateTaskRequest, Task, TaskType};
use crate::infra::persistence::task_repo::TaskRepo;
use crate::infra::queue::task_queue::TaskQueue;
use crate::shared::error::AppError;
use std::sync::Arc;

/// Stateless service that wraps TaskRepo persistence and TaskQueue orchestration.
pub struct TaskService {
    pub task_repo: Arc<dyn TaskRepo>,
    task_queue: Arc<TaskQueue>,
}

impl TaskService {
    pub fn new(task_repo: Arc<dyn TaskRepo>, task_queue: Arc<TaskQueue>) -> Self {
        Self {
            task_repo,
            task_queue,
        }
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

    /// Recover an incomplete task after a restart (cache miss / crash).
    ///
    /// Delegates to `TaskQueue::continue_task`.
    pub async fn continue_task(&self, task_id: &str) -> Result<(), AppError> {
        self.task_queue.continue_task(task_id)
    }
}
