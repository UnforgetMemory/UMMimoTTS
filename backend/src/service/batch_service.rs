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
use crate::infra::persistence::batch_repo::{BatchRepo, ItemOverride, PendingItemRow, SubmitTaskResult};
use crate::infra::sse_bus::SseBus;
use crate::service::task_service::TaskService;
use crate::shared::error::AppError;
use crate::shared::id::Id;
use std::sync::Arc;
use tracing::error;

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
            total_chars: content.chars().count() as i64,
            token_estimate: (content.chars().count() as i64) / 2,
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

    /// Batch-add multiple pending items in a single DB transaction.
    ///
    /// Reads the batch defaults once and applies them to all items.
    pub fn add_items(&self, batch_id: &str, items: &[crate::routes::batches::AddItemRequest]) -> Result<(), AppError> {
        let batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        let pending: Vec<BatchPendingItem> = items
            .iter()
            .map(|req| BatchPendingItem {
                seq: req.seq,
                filename: req.filename.clone(),
                content: req.content.clone(),
                total_chars: req.content.chars().count() as i64,
                token_estimate: (req.content.chars().count() as i64) / 2,
                custom_voice: None,
                custom_model: None,
                custom_title: None,
                custom_style: None,
                custom_speed: None,
                effective_voice: batch.voice.clone(),
                effective_model: batch.model.clone(),
                effective_title: req.filename.clone(),
                effective_style: batch.style.clone(),
                effective_speed: batch.speed,
            })
            .collect();

        self.batch_repo.batch_insert_pending_items(batch_id, &pending)?;
        Ok(())
    }

    /// Delete a batch and cascade to pending items, tasks, and groups.
    pub fn delete(&self, batch_id: &str) -> Result<(), AppError> {
        self.batch_repo.delete_batch(batch_id)
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
    /// 1. Creates `Task` rows from pending items (fast, DB only).
    /// 2. Returns immediately — enqueueing runs in a background tokio task
    ///    (avoids frontend timeout when many items call MIMO chunker HTTP API).
    pub async fn submit(&self, batch_id: &str) -> Result<Vec<TaskSummary>, AppError> {
        // 1. Create child Tasks from pending items (fast, DB only — no content loaded)
        let results: Vec<SubmitTaskResult> = self.batch_repo.submit_batch(batch_id)?;

        // Prepare summaries for immediate return
        let summaries: Vec<TaskSummary> = results
            .iter()
            .map(|r| TaskSummary {
                id: r.id.clone(),
                title: r.title.clone(),
                enqueued: false,
                error: None,
            })
            .collect();

        // 2. Spawn background enqueue so the HTTP response returns quickly
        let task_service = self.task_service.clone();
        let task_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();

        tokio::spawn(async move {
            for (i, task_id) in task_ids.iter().enumerate() {
                // Delay between tasks so the frontend can observe status transitions
                // via SSE events (Pending → Queued → Chunking → Processing).
                if i > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                match task_service.enqueue(task_id).await {
                    Ok(()) => {
                        // TaskEnqueued event is already sent by task_queue.enqueue()
                        // via the event_tx broadcast channel → sse_bridge → sse_bus.
                    }
                    Err(e) => {
                        error!("Failed to enqueue task {task_id}: {e}");
                    }
                }
            }
        });

        Ok(summaries)
    }

    // ── title / fields ──────────────────────────────────────────────

    /// Update the batch title.
    pub fn update_title(&self, batch_id: &str, title: &str) -> Result<(), AppError> {
        self.batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;
        self.batch_repo.update_batch_title(batch_id, title)?;
        Ok(())
    }

    /// Patch multiple batch fields (name/voice/model/style/speed).
    pub fn patch(
        &self,
        batch_id: &str,
        title: Option<&str>,
        voice: Option<&str>,
        model: Option<&str>,
        style: Option<&str>,
        speed: Option<f64>,
    ) -> Result<(), AppError> {
        self.batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;
        self.batch_repo
            .update_batch_fields(batch_id, title, voice, model, style, speed)?;
        Ok(())
    }

    // ── pause / resume ───────────────────────────────────────────────

    /// Pause a batch and all non-terminal child tasks.
    pub fn pause(&self, batch_id: &str) -> Result<(), AppError> {
        use crate::domain::batch::BatchStatus;
        use crate::domain::task::TaskStatus;

        let mut batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        batch
            .transition_to(BatchStatus::Paused)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;
        self.batch_repo
            .update_batch_status(batch_id, &BatchStatus::Paused)?;

        // Pause all running/pending child tasks
        let tasks = self.task_service.get_by_batch(batch_id)?;
        for task in &tasks {
            if matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::Chunking
                    | TaskStatus::Processing
                    | TaskStatus::Merging
                    | TaskStatus::MergingFailed
            ) {
                self.task_service
                    .task_repo
                    .update_status(&task.id.to_string(), &TaskStatus::Paused)?;
            }
        }

        // Notify via SSE
        let batch_id_obj = Id::from_str(batch_id)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.sse_bus.publish(
            &format!("batch:{batch_id}"),
            &DomainEvent::BatchPaused {
                batch_id: batch_id_obj,
            },
        );

        Ok(())
    }

    /// Resume a paused batch — sets paused tasks back to Queued and re-enqueues them.
    pub fn resume(&self, batch_id: &str) -> Result<(), AppError> {
        use crate::domain::batch::BatchStatus;
        use crate::domain::task::TaskStatus;

        let mut batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        batch
            .transition_to(BatchStatus::Processing)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;
        self.batch_repo
            .update_batch_status(batch_id, &BatchStatus::Processing)?;

        // Collect paused tasks and reset them to Queued
        let tasks = self.task_service.get_by_batch(batch_id)?;
        let paused_ids: Vec<String> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Paused)
            .map(|t| t.id.to_string())
            .collect();

        for task_id in &paused_ids {
            self.task_service
                .task_repo
                .update_status(task_id, &TaskStatus::Queued)?;
        }

        // Spawn background enqueue for the resumed tasks
        let task_service = self.task_service.clone();
        let ids = paused_ids.clone();

        tokio::spawn(async move {
            for task_id in &ids {
                match task_service.enqueue(task_id).await {
                    Ok(()) => {
                        // TaskEnqueued event is already sent by task_queue.enqueue()
                    }
                    Err(e) => {
                        error!("Failed to enqueue task {task_id} on resume: {e}");
                    }
                }
            }
        });

        // Notify via SSE
        let batch_id_obj = Id::from_str(batch_id)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.sse_bus.publish(
            &format!("batch:{batch_id}"),
            &DomainEvent::BatchResumed {
                batch_id: batch_id_obj,
            },
        );

        Ok(())
    }

    // ── retry ────────────────────────────────────────────────────────

    /// Retry all failed tasks in a batch.
    pub fn retry_failed(&self, batch_id: &str) -> Result<(), AppError> {
        use crate::domain::batch::BatchStatus;
        use crate::domain::task::TaskStatus;

        // Find and validate the batch
        let mut batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        batch
            .transition_to(BatchStatus::Queued)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;
        self.batch_repo
            .update_batch_status(batch_id, &BatchStatus::Queued)?;

        // Collect failed tasks
        let tasks = self.task_service.get_by_batch(batch_id)?;
        let failed_ids: Vec<String> = tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Failed
                    || t.status == TaskStatus::MergingFailed
            })
            .map(|t| t.id.to_string())
            .collect();

        if failed_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "No failed tasks to retry".into(),
            ));
        }

        // Background retry
        let task_service = self.task_service.clone();

        tokio::spawn(async move {
            for task_id in &failed_ids {
                match task_service.retry(task_id).await {
                    Ok(()) => {
                        // TaskEnqueued event is already sent by task_queue.enqueue()
                    }
                    Err(e) => {
                        error!("Failed to retry task {task_id}: {e}");
                    }
                }
            }
        });

        Ok(())
    }

    // ── download ─────────────────────────────────────────────────────

    /// Build a ZIP archive of all completed task audio files in the batch.
    pub fn download_audio(&self, batch_id: &str) -> Result<Vec<u8>, AppError> {
        use crate::domain::task::TaskStatus;
        use std::io::Write;

        // Verify batch exists
        self.batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        // Collect completed tasks with audio output
        let tasks = self.task_service.get_by_batch(batch_id)?;
        let completed: Vec<&crate::domain::task::Task> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done && t.output_path.is_some())
            .collect();

        if completed.is_empty() {
            return Err(AppError::InvalidInput(
                "No completed tasks with audio available".into(),
            ));
        }

        // Build the ZIP in memory
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip_writer = zip::ZipWriter::new(cursor);
        let options =
            zip::write::FileOptions::default().compression_method(
                zip::CompressionMethod::Deflated,
            );

        for task in &completed {
            let path = task.output_path.as_ref().unwrap();
            let safe_name = sanitize_filename::sanitize(&task.title);
            let filename = format!("{}_{}.wav", safe_name, task.id);

            match std::fs::read(path) {
                Ok(data) => {
                    zip_writer
                        .start_file(&filename, options)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                    zip_writer
                        .write_all(&data)
                        .map_err(|e| AppError::Internal(e.to_string()))?;
                }
                Err(e) => {
                    tracing::warn!(
                        "Could not read audio for task {} ({}): {}",
                        task.id,
                        path,
                        e
                    );
                }
            }
        }

        let finished = zip_writer
            .finish()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(finished.into_inner())
    }

    // ── cancel / cancel-all ──────────────────────────────────────────

    /// Cancel a batch and all its non-terminal child tasks.
    pub fn cancel(&self, batch_id: &str) -> Result<(), AppError> {
        use crate::domain::batch::BatchStatus;
        use crate::domain::task::TaskStatus;

        let mut batch = self
            .batch_repo
            .find_batch(batch_id)?
            .ok_or_else(|| AppError::NotFound(format!("Batch {batch_id}")))?;

        batch
            .transition_to(BatchStatus::Cancelled)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;
        self.batch_repo
            .update_batch_status(batch_id, &BatchStatus::Cancelled)?;

        // Cancel all running/pending child tasks
        let tasks = self.task_service.get_by_batch(batch_id)?;
        for task in &tasks {
            if matches!(
                task.status,
                TaskStatus::Pending
                    | TaskStatus::Queued
                    | TaskStatus::Chunking
                    | TaskStatus::Processing
                    | TaskStatus::Merging
                    | TaskStatus::MergingFailed
                    | TaskStatus::Paused
                    | TaskStatus::Failed
            ) {
                let _ = self.task_service.cancel(&task.id.to_string());
            }
        }

        // Notify via SSE
        let batch_id_obj = Id::from_str(batch_id)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.sse_bus.publish(
            &format!("batch:{batch_id}"),
            &DomainEvent::BatchPaused {
                batch_id: batch_id_obj,
            },
        );

        Ok(())
    }

    /// Cancel ALL non-terminal tasks across all batches.
    pub fn cancel_all(&self) -> Result<(), AppError> {
        use crate::domain::task::TaskStatus;

        let all_tasks = self.task_service.task_repo.find_all()?;
        for task in &all_tasks {
            if matches!(
                task.status,
                TaskStatus::Pending
                    | TaskStatus::Queued
                    | TaskStatus::Chunking
                    | TaskStatus::Processing
                    | TaskStatus::Merging
                    | TaskStatus::MergingFailed
                    | TaskStatus::Paused
                    | TaskStatus::Failed
            ) {
                let _ = self.task_service.cancel(&task.id.to_string());
            }
        }

        Ok(())
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
