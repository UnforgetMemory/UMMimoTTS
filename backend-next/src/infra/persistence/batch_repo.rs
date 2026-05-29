//! Batch repository trait and SQLite implementation.

#![allow(dead_code)]

use crate::shared::id::Id;
use crate::shared::error::AppError;
use crate::domain::batch::{Batch, BatchStatus, BatchPendingItem};
use crate::domain::task::{Task, TaskType, CreateTaskRequest};
use crate::infra::persistence::db::DbPool;
use chrono::Utc;
use rusqlite::{params, Connection, TransactionBehavior};
use serde::{Serialize, Deserialize};

/// Full row from the pending_items table, including DB-managed fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingItemRow {
    pub id: String,
    pub batch_id: String,
    pub seq: i32,
    pub filename: String,
    pub content: String,
    pub text_preview: String,
    pub total_chars: i64,
    pub token_estimate: i64,
    pub custom_title: Option<String>,
    pub custom_voice: Option<String>,
    pub custom_model: Option<String>,
    pub custom_style: Option<String>,
    pub custom_speed: Option<f64>,
    pub effective_title: String,
    pub effective_voice: String,
    pub effective_model: String,
    pub effective_style: Option<String>,
    pub effective_speed: f64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Paginated result for pending items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedItems {
    pub items: Vec<PendingItemRow>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// Override values that can be applied to a pending item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemOverride {
    pub title: Option<String>,
    pub voice: Option<String>,
    pub model: Option<String>,
    pub style: Option<String>,
    pub speed: Option<f64>,
}

/// Default values inherited from the batch.
pub struct BatchDefaults {
    pub voice: String,
    pub model: String,
    pub speed: f64,
}

pub trait BatchRepo: Send + Sync {
    fn insert_batch(&self, batch: &Batch) -> Result<(), AppError>;
    fn find_batch(&self, id: &str) -> Result<Option<Batch>, AppError>;
    fn update_batch_status(&self, id: &str, status: &BatchStatus) -> Result<(), AppError>;
    fn delete_batch(&self, id: &str) -> Result<(), AppError>;
    fn insert_pending_item(&self, batch_id: &str, item: &BatchPendingItem) -> Result<(), AppError>;
    fn batch_insert_pending_items(&self, batch_id: &str, items: &[BatchPendingItem]) -> Result<(), AppError>;
    fn list_pending_items(
        &self,
        batch_id: &str,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedItems, AppError>;
    fn find_pending_item_by_seq(
        &self,
        batch_id: &str,
        seq: i32,
    ) -> Result<Option<PendingItemRow>, AppError>;
    fn update_pending_item(&self, id: &str, item: &BatchPendingItem) -> Result<(), AppError>;
    fn batch_update_pending_items(
        &self,
        batch_id: &str,
        items: &[BatchPendingItem],
    ) -> Result<(), AppError>;
    fn delete_pending_item(&self, id: &str) -> Result<(), AppError>;
    fn count_pending_items(&self, batch_id: &str) -> Result<i64, AppError>;
    fn submit_batch(&self, batch_id: &str) -> Result<Vec<SubmitTaskResult>, AppError>;
    fn get_child_task_ids(&self, batch_id: &str) -> Result<Vec<String>, AppError>;
    fn list_all(&self) -> Result<Vec<Batch>, AppError>;
}

/// Minimal task info returned from `submit_batch` — avoids loading full content into memory.
#[derive(Debug, Clone)]
pub struct SubmitTaskResult {
    pub id: String,
    pub title: String,
}

pub struct SqliteBatchRepo {
    pub pool: DbPool,
}

impl SqliteBatchRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn row_to_batch(row: &rusqlite::Row) -> rusqlite::Result<Batch> {
        Ok(Batch {
            id: Id::from_str(&row.get::<_, String>("id")?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            status: serde_json::from_str(&row.get::<_, String>("status")?).unwrap(),
            title: row.get::<_, String>("name")?,
            voice: row.get::<_, String>("default_voice")?,
            model: row.get::<_, String>("default_model")?,
            style: row.get("default_style")?,
            speed: row.get("default_speed")?,
            total_items: row.get::<_, i32>("total_tasks")?,
            total_chars: row.get::<_, i64>("total_chars")?,
            total_tokens: row.get::<_, i64>("total_tokens")?,
            created_at: parse_datetime(&row.get::<_, String>("created_at")?),
            updated_at: parse_datetime(&row.get::<_, String>("updated_at")?),
            completed_at: row
                .get::<_, Option<String>>("completed_at")?
                .map(|s| parse_datetime(&s)),
        })
    }

    fn row_to_pending_item(row: &rusqlite::Row) -> rusqlite::Result<PendingItemRow> {
        Ok(PendingItemRow {
            id: row.get("id")?,
            batch_id: row.get("batch_id")?,
            seq: row.get("seq")?,
            filename: row.get("filename")?,
            content: row.get("content")?,
            text_preview: row.get("text_preview")?,
            total_chars: row.get("total_chars")?,
            token_estimate: row.get("token_estimate")?,
            custom_title: row.get("custom_title")?,
            custom_voice: row.get("custom_voice")?,
            custom_model: row.get("custom_model")?,
            custom_style: row.get("custom_style")?,
            custom_speed: row.get("custom_speed")?,
            effective_title: row.get("effective_title")?,
            effective_voice: row.get("effective_voice")?,
            effective_model: row.get("effective_model")?,
            effective_style: row.get("effective_style")?,
            effective_speed: row.get("effective_speed")?,
            status: row.get("status")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    fn submit_batch_inner(
        conn: &mut Connection,
        batch_id: &str,
    ) -> Result<Vec<SubmitTaskResult>, AppError> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Read batch to verify it exists
        let _batch: Batch = {
            let mut stmt = tx.prepare("SELECT * FROM batches WHERE id = ?1")?;
            stmt.query_row(params![batch_id], Self::row_to_batch)?
        };

        // Read all pending items for this batch
        let items: Vec<PendingItemRow> = {
            let mut stmt = tx.prepare(
                "SELECT * FROM pending_items WHERE batch_id = ?1 AND status = 'pending' ORDER BY seq",
            )?;
            let rows = stmt.query_map(params![batch_id], Self::row_to_pending_item)?;
            rows.filter_map(|r| r.ok()).collect()
        };

        if items.is_empty() {
            return Err(AppError::InvalidInput(
                "No pending items to submit".into(),
            ));
        }

        // Create a Task for each pending item (temporary — INSERT only, then dropped)
        let mut results = Vec::with_capacity(items.len());
        let total_items = items.len() as i32;
        let total_tokens_sum: i64 = items.iter().map(|i| i.token_estimate).sum();
        let total_chars_sum: i64 = items.iter().map(|i| i.total_chars).sum();

        for item in &items {
            let batch_id_obj = Id::from_str(batch_id)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            let mut task = Task::new(CreateTaskRequest {
                task_type: TaskType::BatchChild,
                batch_id: Some(batch_id_obj.clone()),
                content: item.content.clone(),
                content_ref: None,
                title: item.effective_title.clone(),
                voice: item.effective_voice.clone(),
                model: item.effective_model.clone(),
                style: item.effective_style.clone(),
                speed: item.effective_speed,
                total_chars: item.total_chars,
                total_tokens: item.token_estimate,
            });
            // Set group_id = batch_id (batch and group share the same ID)
            task.group_id = Some(batch_id_obj);

            // Insert task
            tx.execute(
                "INSERT INTO tasks (id, task_type, status, group_id, batch_id, content, content_ref,
                 title, voice, model, style, speed, priority, total_chars, total_tokens,
                 total_chunks, done_chunks, failed_chunks, output_path, output_duration,
                 created_at, updated_at, completed_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0,?13,?14,0,0,0,NULL,0,?15,?16,NULL)",
                params![
                    task.id.to_string(),
                    serde_json::to_string(&task.task_type).unwrap(),
                    serde_json::to_string(&task.status).unwrap(),
                    task.group_id.as_ref().map(|id| id.to_string()),
                    task.batch_id.as_ref().map(|id| id.to_string()),
                    task.content,
                    task.content_ref,
                    task.title,
                    task.voice,
                    task.model,
                    task.style,
                    task.speed,
                    task.total_chars,
                    task.total_tokens,
                    task.created_at.to_rfc3339(),
                    task.updated_at.to_rfc3339(),
                ],
            )?;

            // Insert batch_tasks record
            tx.execute(
                "INSERT INTO batch_tasks (id, batch_id, child_task_id, seq) VALUES (?1,?2,?3,?4)",
                params![
                    Id::new().to_string(),
                    batch_id,
                    task.id.to_string(),
                    item.seq,
                ],
            )?;

            results.push(SubmitTaskResult {
                id: task.id.to_string(),
                title: task.title.clone(),
            });
        }

        // Clear pending items
        tx.execute(
            "UPDATE pending_items SET status = 'converted', updated_at = ?1 WHERE batch_id = ?2 AND status = 'pending'",
            params![Utc::now().to_rfc3339(), batch_id],
        )?;

        // Update batch status and totals
        let now_rfc = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE batches SET status = ?1, total_tasks = ?2, total_tokens = ?3, total_chars = ?4, updated_at = ?5 WHERE id = ?6",
            params![
                serde_json::to_string(&BatchStatus::Queued).unwrap(),
                total_items,
                total_tokens_sum,
                total_chars_sum,
                now_rfc,
                batch_id,
            ],
        )?;

        tx.commit()?;
        Ok(results)
    }
}

fn parse_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

impl BatchRepo for SqliteBatchRepo {
    fn insert_batch(&self, batch: &Batch) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO batches (id, name, status, total_tasks, done_tasks, failed_tasks,
             default_voice, default_model, default_style, default_speed,
             created_at, updated_at, completed_at)
             VALUES (?1,?2,?3,0,0,0,?4,?5,?6,?7,?8,?9,?10)",
            params![
                batch.id.to_string(),
                batch.title,
                serde_json::to_string(&batch.status).unwrap(),
                batch.voice,
                batch.model,
                batch.style,
                batch.speed,
                batch.created_at.to_rfc3339(),
                batch.updated_at.to_rfc3339(),
                batch.completed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    fn find_batch(&self, id: &str) -> Result<Option<Batch>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM batches WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], Self::row_to_batch)?;
        match rows.next() {
            Some(Ok(batch)) => Ok(Some(batch)),
            _ => Ok(None),
        }
    }

    fn update_batch_status(&self, id: &str, status: &BatchStatus) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let is_terminal = matches!(status, BatchStatus::Completed | BatchStatus::Cancelled | BatchStatus::Failed);
        let completed_at = if is_terminal {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        conn.execute(
            "UPDATE batches SET status = ?1, updated_at = ?2, completed_at = COALESCE(?3, completed_at) WHERE id = ?4",
            params![
                serde_json::to_string(status).unwrap(),
                Utc::now().to_rfc3339(),
                completed_at,
                id,
            ],
        )?;
        Ok(())
    }

    fn delete_batch(&self, id: &str) -> Result<(), AppError> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Order matters: FK constraints must be satisfied
        // chunks → tasks → batch_tasks → pending_items/groups → batches
        tx.execute(
            "DELETE FROM chunks WHERE task_id IN (SELECT id FROM tasks WHERE batch_id = ?1)",
            params![id],
        )?;
        tx.execute("DELETE FROM pending_items WHERE batch_id = ?1", params![id])?;
        tx.execute("DELETE FROM batch_tasks WHERE batch_id = ?1", params![id])?;
        tx.execute("DELETE FROM tasks WHERE batch_id = ?1", params![id])?;
        tx.execute("DELETE FROM groups WHERE batch_id = ?1", params![id])?;
        tx.execute("DELETE FROM batches WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    fn insert_pending_item(&self, batch_id: &str, item: &BatchPendingItem) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let now = Utc::now().to_rfc3339();
        let text_preview: String = item.content.chars().take(100).collect();
        let text_preview = if item.content.chars().count() > 100 {
            format!("{}...", text_preview)
        } else {
            text_preview
        };
        conn.execute(
            "INSERT INTO pending_items (id, batch_id, seq, filename, content, text_preview,
             total_chars, token_estimate, custom_title, custom_voice, custom_model, custom_style, custom_speed,
             effective_title, effective_voice, effective_model, effective_style, effective_speed,
             status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,'pending',?19,?20)",
            params![
                Id::new().to_string(),
                batch_id,
                item.seq,
                item.filename,
                item.content,
                text_preview,
                item.total_chars,
                item.token_estimate,
                item.custom_title,
                item.custom_voice,
                item.custom_model,
                item.custom_style,
                item.custom_speed,
                item.effective_title,
                item.effective_voice,
                item.effective_model,
                item.effective_style,
                item.effective_speed,
                now,
                now,
            ],
        )?;
        Ok(())
    }

    fn batch_insert_pending_items(
        &self,
        batch_id: &str,
        items: &[BatchPendingItem],
    ) -> Result<(), AppError> {
        let mut conn = self.pool.get()?;
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for item in items {
            let text_preview: String = item.content.chars().take(100).collect();
            let text_preview = if item.content.chars().count() > 100 {
                format!("{}...", text_preview)
            } else {
                text_preview
            };
            tx.execute(
                "INSERT INTO pending_items (id, batch_id, seq, filename, content, text_preview,
                 total_chars, token_estimate, custom_title, custom_voice, custom_model, custom_style, custom_speed,
                 effective_title, effective_voice, effective_model, effective_style, effective_speed,
                 status, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,'pending',?19,?20)",
                params![
                    Id::new().to_string(),
                    batch_id,
                    item.seq,
                    item.filename,
                    item.content,
                    text_preview,
                    item.total_chars,
                    item.token_estimate,
                    item.custom_title,
                    item.custom_voice,
                    item.custom_model,
                    item.custom_style,
                    item.custom_speed,
                    item.effective_title,
                    item.effective_voice,
                    item.effective_model,
                    item.effective_style,
                    item.effective_speed,
                    now,
                    now,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn list_pending_items(
        &self,
        batch_id: &str,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedItems, AppError> {
        let conn = self.pool.get()?;
        let offset = (page - 1).max(0) * per_page;

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pending_items WHERE batch_id = ?1 AND status = 'pending'",
            params![batch_id],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT * FROM pending_items WHERE batch_id = ?1 AND status = 'pending' ORDER BY seq ASC LIMIT ?2 OFFSET ?3",
        )?;
        let items: Vec<PendingItemRow> = stmt
            .query_map(params![batch_id, per_page, offset], Self::row_to_pending_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(PaginatedItems {
            items,
            total,
            page,
            per_page,
        })
    }

    fn find_pending_item_by_seq(
        &self,
        batch_id: &str,
        seq: i32,
    ) -> Result<Option<PendingItemRow>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM pending_items WHERE batch_id = ?1 AND seq = ?2 AND status = 'pending'",
        )?;
        let mut rows = stmt.query_map(params![batch_id, seq], Self::row_to_pending_item)?;
        match rows.next() {
            Some(Ok(item)) => Ok(Some(item)),
            _ => Ok(None),
        }
    }

    fn update_pending_item(&self, id: &str, item: &BatchPendingItem) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE pending_items SET seq = ?1, filename = ?2, content = ?3,
             text_preview = ?4, total_chars = ?5, token_estimate = ?6,
             custom_title = ?7, custom_voice = ?8, custom_model = ?9, custom_style = ?10, custom_speed = ?11,
             effective_title = ?12, effective_voice = ?13, effective_model = ?14, effective_style = ?15, effective_speed = ?16,
             updated_at = ?17 WHERE id = ?18",
            params![
                item.seq,
                item.filename,
                item.content,
                if item.content.len() > 100 {
                    format!("{}...", &item.content[..100])
                } else {
                    item.content.clone()
                },
                item.total_chars,
                item.token_estimate,
                item.custom_title,
                item.custom_voice,
                item.custom_model,
                item.custom_style,
                item.custom_speed,
                item.effective_title,
                item.effective_voice,
                item.effective_model,
                item.effective_style,
                item.effective_speed,
                now,
                id,
            ],
        )?;
        Ok(())
    }

    fn batch_update_pending_items(
        &self,
        batch_id: &str,
        items: &[BatchPendingItem],
    ) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let tx = conn.unchecked_transaction()?;
        // Delete existing pending items for this batch
        tx.execute(
            "DELETE FROM pending_items WHERE batch_id = ?1 AND status = 'pending'",
            params![batch_id],
        )?;
        // Re-insert all items
        let now = Utc::now().to_rfc3339();
        for item in items {
            let text_preview = if item.content.len() > 100 {
                format!("{}...", &item.content[..100])
            } else {
                item.content.clone()
            };
            tx.execute(
                "INSERT INTO pending_items (id, batch_id, seq, filename, content, text_preview,
                 total_chars, token_estimate,
                 custom_title, custom_voice, custom_model, custom_style, custom_speed,
                 effective_title, effective_voice, effective_model, effective_style, effective_speed,
                 status, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,'pending',?19,?20)",
                params![
                    Id::new().to_string(),
                    batch_id,
                    item.seq,
                    item.filename,
                    item.content,
                    text_preview,
                    item.total_chars,
                    item.token_estimate,
                    item.custom_title,
                    item.custom_voice,
                    item.custom_model,
                    item.custom_style,
                    item.custom_speed,
                    item.effective_title,
                    item.effective_voice,
                    item.effective_model,
                    item.effective_style,
                    item.effective_speed,
                    now,
                    now,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn delete_pending_item(&self, id: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute("DELETE FROM pending_items WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn count_pending_items(&self, batch_id: &str) -> Result<i64, AppError> {
        let conn = self.pool.get()?;
        let count = conn.query_row(
            "SELECT COUNT(*) FROM pending_items WHERE batch_id = ?1 AND status = 'pending'",
            params![batch_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count)
    }

    fn submit_batch(&self, batch_id: &str) -> Result<Vec<SubmitTaskResult>, AppError> {
        let max_retries = 3;

        for attempt in 0..max_retries {
            let mut conn = self.pool.get()?;

            let result = Self::submit_batch_inner(&mut *conn, batch_id);

            match result {
                Ok(results) => return Ok(results),
                Err(AppError::Internal(_)) if attempt < max_retries - 1 => {
                    std::thread::sleep(std::time::Duration::from_millis(
                        10 * 2u64.pow(attempt as u32),
                    ));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(AppError::Internal(
            "Failed to submit batch after retries".into(),
        ))
    }

    fn get_child_task_ids(&self, batch_id: &str) -> Result<Vec<String>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT child_task_id FROM batch_tasks WHERE batch_id = ?1 ORDER BY seq",
        )?;
        let ids: Vec<String> = stmt
            .query_map(params![batch_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    fn list_all(&self) -> Result<Vec<Batch>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM batches ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_batch)?;
        let batches: Vec<Batch> = rows.filter_map(|r| r.ok()).collect();
        Ok(batches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::batch::{Batch, BatchPendingItem};
    use crate::infra::persistence::db::create_test_pool;
    use crate::infra::persistence::migrate::run_migrations;

    fn create_test_batch() -> Batch {
        Batch::new("Test Batch".into(), "v1".into(), "m1".into(), None, 1.0)
    }

    fn create_test_item(batch: &Batch, seq: i32, content: &str) -> BatchPendingItem {
        let mut item = BatchPendingItem::new_for_test(batch, &format!("file{}.txt", seq), content, None);
        item.seq = seq;
        item
    }

    #[test]
    fn test_batch_insert_and_find() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();
        let found = repo.find_batch(batch.id.as_str()).unwrap().unwrap();
        assert_eq!(found.title, batch.title);
        assert_eq!(found.voice, batch.voice);
        assert_eq!(found.model, batch.model);
        assert_eq!(found.status, BatchStatus::Preparing);
    }

    #[test]
    fn test_batch_update_status() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();
        repo.update_batch_status(batch.id.as_str(), &BatchStatus::Queued)
            .unwrap();
        let found = repo.find_batch(batch.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, BatchStatus::Queued);
    }

    #[test]
    fn test_batch_delete() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();
        repo.delete_batch(batch.id.as_str()).unwrap();
        let found = repo.find_batch(batch.id.as_str()).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_pending_item_crud() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();

        // Insert items
        for i in 0..3 {
            let item = create_test_item(&batch, i, &format!("content {}", i));
            repo.insert_pending_item(batch.id.as_str(), &item)
                .unwrap();
        }

        // List items
        let paginated = repo
            .list_pending_items(batch.id.as_str(), 1, 10)
            .unwrap();
        assert_eq!(paginated.total, 3);
        assert_eq!(paginated.items.len(), 3);
        assert_eq!(paginated.items[0].seq, 0);

        // Count items
        let count = repo.count_pending_items(batch.id.as_str()).unwrap();
        assert_eq!(count, 3);

        // Find by seq
        let item = repo
            .find_pending_item_by_seq(batch.id.as_str(), 1)
            .unwrap()
            .unwrap();
        assert_eq!(item.seq, 1);
        assert_eq!(item.content, "content 1");

        // Delete item
        repo.delete_pending_item(&paginated.items[0].id).unwrap();
        let count = repo.count_pending_items(batch.id.as_str()).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_batch_update_pending_items() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();

        // Insert initial items
        let item0 = create_test_item(&batch, 0, "original");
        repo.insert_pending_item(batch.id.as_str(), &item0).unwrap();

        // Batch update replaces all items
        let items: Vec<BatchPendingItem> = (0..2)
            .map(|i| create_test_item(&batch, i, "new content"))
            .collect();
        repo.batch_update_pending_items(batch.id.as_str(), &items)
            .unwrap();
        let count = repo.count_pending_items(batch.id.as_str()).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_submit_batch() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();

        // Add pending items
        for i in 0..3 {
            let item = create_test_item(&batch, i, &format!("content {}", i));
            repo.insert_pending_item(batch.id.as_str(), &item)
                .unwrap();
        }

        // Submit the batch
        let tasks = repo.submit_batch(batch.id.as_str()).unwrap();
        assert_eq!(tasks.len(), 3);

        // Verify batch status updated
        let found = repo.find_batch(batch.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, BatchStatus::Queued);
        assert_eq!(found.total_items, 3);

        // Verify child task IDs
        let child_ids = repo.get_child_task_ids(batch.id.as_str()).unwrap();
        assert_eq!(child_ids.len(), 3);

        // Verify pending items converted
        let count = repo.count_pending_items(batch.id.as_str()).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_submit_batch_empty_fails() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();

        let result = repo.submit_batch(batch.id.as_str());
        assert!(result.is_err());
    }

    #[test]
    fn test_pending_item_pagination() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteBatchRepo::new(pool);
        let batch = create_test_batch();
        repo.insert_batch(&batch).unwrap();

        for i in 0..10 {
            let item = create_test_item(&batch, i, &format!("content {}", i));
            repo.insert_pending_item(batch.id.as_str(), &item)
                .unwrap();
        }

        let page1 = repo
            .list_pending_items(batch.id.as_str(), 1, 5)
            .unwrap();
        assert_eq!(page1.items.len(), 5);
        assert_eq!(page1.total, 10);

        let page2 = repo
            .list_pending_items(batch.id.as_str(), 2, 5)
            .unwrap();
        assert_eq!(page2.items.len(), 5);
        assert_eq!(page2.items[0].seq, 5);
    }
}
