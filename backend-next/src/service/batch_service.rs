//! Batch service — wraps BatchRepo + TaskService + SseBus.
//!
//! Manages the end-to-end lifecycle of a batch import:
//! - `create` — new batch with default TTS params
//! - `add_item` — upload a single file's content as a pending item
//! - `update_item` — apply overrides to a pending item
//! - `submit` — finalise the batch, create child Tasks, enqueue them

#![allow(dead_code)]

use crate::domain::batch::{Batch, BatchPendingItem};
use crate::domain::events::DomainEvent;
use crate::infra::persistence::batch_repo::{BatchRepo, ItemOverride, PendingItemRow};
use crate::infra::sse_bus::SseBus;
use crate::service::task_service::TaskService;
use crate::domain::task::Task;
use crate::shared::error::AppError;
use crate::shared::id::Id;
use std::sync::Arc;

/// Stateless service wrapping batch persistence + task orchestration.
pub struct BatchService {
    pub batch_repo: Arc<dyn BatchRepo>,
    task_service: Arc<TaskService>,
    sse_bus: Arc<SseBus>,
}

impl BatchService {
    pub fn new(
        batch_repo: Arc<dyn BatchRepo>,
        task_service: Arc<TaskService>,
        sse_bus: Arc<SseBus>,
    ) -> Self {
        Self {
            batch_repo,
            task_service,
            sse_bus,
        }
    }

    // ── lifecycle ───────────────────────────────────────────────────

    /// Create a new batch with the given default TTS parameters.
    pub fn create(
        &self,
        title: String,
        voice: String,
        model: String,
        style: Option<String>,
        speed: f64,
    ) -> Result<Batch, AppError> {
        let batch = Batch::new(title, voice, model, style, speed);
        self.batch_repo.insert_batch(&batch)?;
        Ok(batch)
    }

    // ── pending-items ───────────────────────────────────────────────

    /// Add a single pending item (representing one uploaded file).
    ///
    /// The service reads the batch defaults and computes the effective
    /// voice / model / title / style / speed for this item, inheriting
    /// from the batch when no custom override is set.
    pub fn add_item(
        &self,
        batch_id: &str,
        seq: i32,
        filename: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        let item = BatchPendingItem {
            seq,
            filename: filename.to_string(),
            content: content.to_string(),
            total_chars: content.len() as i64,
            token_estimate: (content.len() as i64) / 2,
            custom_voice: None,
            custom_model: None,
            custom_title: None,
            custom_style: None,
            custom_speed: None,
            effective_voice: batch.voice.clone(),
            effective_model: batch.model.clone(),
            effective_title: filename.to_string(),
            effective_style: batch.style.clone(),
            effective_speed: batch.speed,
        };

        self.batch_repo.insert_pending_item(batch_id, &item)?;
        Ok(())
    }

    /// Update a pending item's overrides.
    ///
    /// Merges the provided overrides with the existing item values and
    /// recomputes effective fields.
    pub fn update_item(
        &self,
        batch_id: &str,
        seq: i32,
        overrides: &ItemOverride,
    ) -> Result<PendingItemRow, AppError> {
        let batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        let current = self
            .batch_repo
            .find_pending_item_by_seq(batch_id, seq)?
            .ok_or_else(|| AppError::NotFound(format!("Item seq={seq} in batch {batch_id}")))?;

        let custom_voice = overrides.voice.clone().or(current.custom_voice);
        let custom_model = overrides.model.clone().or(current.custom_model);
        let custom_title = overrides.title.clone().or(current.custom_title);
        let custom_style = overrides.style.clone().or(current.custom_style);
        let custom_speed = overrides.speed.or(current.custom_speed);

        let updated = BatchPendingItem {
            seq: current.seq,
            filename: current.filename.clone(),
            content: current.content.clone(),
            total_chars: current.total_chars,
            token_estimate: current.token_estimate,
            custom_voice: custom_voice.clone(),
            custom_model: custom_model.clone(),
            custom_title: custom_title.clone(),
            custom_style: custom_style.clone(),
            custom_speed,
            effective_voice: custom_voice
                .clone()
                .unwrap_or_else(|| batch.voice.clone()),
            effective_model: custom_model
                .clone()
                .unwrap_or_else(|| batch.model.clone()),
            effective_title: custom_title
                .clone()
                .unwrap_or_else(|| current.filename.clone()),
            effective_style: custom_style.clone().or(batch.style.clone()),
            effective_speed: custom_speed.unwrap_or(batch.speed),
        };

        self.batch_repo.update_pending_item(&current.id, &updated)?;

        // Return the row after save
        self.batch_repo
            .find_pending_item_by_seq(batch_id, seq)?
            .ok_or_else(|| AppError::Internal("Item vanished after update".into()))
    }

    // ── submission ──────────────────────────────────────────────────

    /// Submit a batch for processing.
    ///
    /// 1. Calls `BatchRepo::submit_batch` which creates `Task` rows from pending items.
    /// 2. Enqueues each task via `TaskService::enqueue`.
    /// 3. Publishes batch-submitted domain events.
    pub async fn submit(&self, batch_id: &str) -> Result<Vec<TaskSummary>, AppError> {
        // 1. Create child Tasks from pending items
        let tasks: Vec<Task> = self.batch_repo.submit_batch(batch_id)?;

        // 2. Enqueue each task
        let mut results = Vec::with_capacity(tasks.len());
        for task in &tasks {
            match self.task_service.enqueue(&task.id.to_string()).await {
                Ok(()) => {
                    self.sse_bus.publish(
                        &format!("batch:{batch_id}"),
                        &DomainEvent::TaskEnqueued {
                            task_id: task.id.clone(),
                            batch_id: Some(Id::from_str(batch_id).unwrap()),
                        },
                    );
                    results.push(TaskSummary {
                        id: task.id.to_string(),
                        title: task.title.clone(),
                        enqueued: true,
                        error: None,
                    });
                }
                Err(e) => {
                    results.push(TaskSummary {
                        id: task.id.to_string(),
                        title: task.title.clone(),
                        enqueued: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(results)
    }
}

/// Lightweight result returned by `submit()` for each child task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub enqueued: bool,
    pub error: Option<String>,
}
