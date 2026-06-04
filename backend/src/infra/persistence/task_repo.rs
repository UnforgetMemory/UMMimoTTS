//! Task repository trait and SQLite implementation.

#![allow(dead_code)]

use crate::shared::id::Id;
use crate::shared::error::AppError;
use crate::domain::task::{Task, TaskStatus};
use crate::infra::persistence::db::DbPool;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Serialize, Deserialize};

/// Aggregate progress for a batch's child tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgressAggregate {
    pub batch_id: String,
    pub total_tasks: i32,
    pub done_tasks: i32,
    pub failed_tasks: i32,
    pub processing_tasks: i32,
}

pub trait TaskRepo: Send + Sync {
    fn insert(&self, task: &Task) -> Result<(), AppError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Task>, AppError>;
    fn update_status(&self, id: &str, status: &TaskStatus) -> Result<(), AppError>;
    fn update_chunk_progress(&self, id: &str, total: i32, done: i32, failed: i32, total_tokens: i64) -> Result<(), AppError>;
    fn update_title(&self, id: &str, title: &str) -> Result<(), AppError>;
    fn set_output(&self, id: &str, path: &str, duration: f64) -> Result<(), AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
    /// Delete ALL tasks and their associated chunks in a single transaction.
    fn delete_all(&self) -> Result<(), AppError>;
    fn find_by_batch(&self, batch_id: &str) -> Result<Vec<Task>, AppError>;
    fn find_by_group(&self, group_id: &str) -> Result<Vec<Task>, AppError>;
    fn batch_progress(&self, batch_id: &str) -> Result<BatchProgressAggregate, AppError>;
    fn find_all(&self) -> Result<Vec<Task>, AppError>;
    fn find_standalone(&self) -> Result<Vec<Task>, AppError>;
    /// Find tasks stuck in Processing status for more than `stale_minutes`.
    /// Returns tasks that have no active (pending/processing) chunks.
    fn find_stale_processing(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError>;
    /// Find tasks stuck in Merging status for more than `stale_minutes`.
    fn find_stale_merging(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError>;
    /// Find tasks stuck in Pending status for more than `stale_minutes`.
    /// These tasks were created but never enqueued — candidates for auto-enqueue.
    fn find_stale_pending(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError>;
    /// Find tasks in Queued or Chunking status that have zero associated chunks.
    /// These tasks failed during chunk insertion — candidates for re-enqueue.
    fn find_stuck_queued(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError>;
    /// Find tasks in Processing status where all chunks are terminal (Done/Failed)
    /// but the task status was not updated. Candidates for re-emitting AllChunksDone.
    fn find_processing_all_done(&self) -> Result<Vec<Task>, AppError>;
}

pub struct SqliteTaskRepo {
    pub pool: DbPool,
}

impl SqliteTaskRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn parse_datetime(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
        Ok(Task {
            id: Id::from_str(&row.get::<_, String>("id")?).unwrap_or_else(|_| Id::new()),
            task_type: serde_json::from_str(&row.get::<_, String>("task_type")?).unwrap(),
            status: serde_json::from_str(&row.get::<_, String>("status")?).unwrap(),
            batch_id: row
                .get::<_, Option<String>>("batch_id")?
                .map(|s| Id::from_str(&s).unwrap()),
            group_id: row
                .get::<_, Option<String>>("group_id")?
                .map(|s| Id::from_str(&s).unwrap()),
            content: row.get("content")?,
            content_ref: row.get("content_ref")?,
            title: row.get("title")?,
            voice: row.get("voice")?,
            model: row.get("model")?,
            style: row.get("style")?,
            speed: row.get("speed")?,
            provider_id: row.get::<_, Option<String>>("provider_id")?,
            total_chars: row.get("total_chars")?,
            total_tokens: row.get("total_tokens")?,
            total_chunks: row.get("total_chunks")?,
            done_chunks: row.get("done_chunks")?,
            failed_chunks: row.get("failed_chunks")?,
            output_path: row.get("output_path")?,
            created_at: Self::parse_datetime(&row.get::<_, String>("created_at")?),
            updated_at: Self::parse_datetime(&row.get::<_, String>("updated_at")?),
            completed_at: row
                .get::<_, Option<String>>("completed_at")?
                .map(|s| Self::parse_datetime(&s)),
        })
    }
}

impl TaskRepo for SqliteTaskRepo {
    fn insert(&self, task: &Task) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO tasks (id, task_type, status, group_id, batch_id, content, content_ref,
             title, voice, model, style, speed, provider_id, priority, total_chars, total_tokens,
             total_chunks, done_chunks, failed_chunks, output_path, output_duration,
             created_at, updated_at, completed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,0,?14,?15,?16,?17,?18,?19,0,?20,?21,?22)",
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
                task.provider_id,
                task.total_chars,
                task.total_tokens,
                task.total_chunks,
                task.done_chunks,
                task.failed_chunks,
                task.output_path,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
                task.completed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    fn find_by_id(&self, id: &str) -> Result<Option<Task>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM tasks WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], Self::row_to_task)?;
        match rows.next() {
            Some(Ok(task)) => Ok(Some(task)),
            _ => Ok(None),
        }
    }

    fn update_status(&self, id: &str, status: &TaskStatus) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let affected = conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![serde_json::to_string(status).unwrap(), Utc::now().to_rfc3339(), id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("Task {} not found", id)));
        }
        Ok(())
    }

    fn update_chunk_progress(&self, id: &str, total: i32, done: i32, failed: i32, total_tokens: i64) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        // Only set total_tokens when > 0 to avoid resetting an existing value
        conn.execute(
            "UPDATE tasks SET total_chunks = ?1, done_chunks = ?2, failed_chunks = ?3,
             total_tokens = CASE WHEN ?4 > 0 THEN ?4 ELSE total_tokens END,
             updated_at = ?5 WHERE id = ?6",
            params![total, done, failed, total_tokens, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn set_output(&self, id: &str, path: &str, duration: f64) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE tasks SET output_path = ?1, output_duration = ?2, updated_at = ?3 WHERE id = ?4",
            params![path, duration, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn find_by_batch(&self, batch_id: &str) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT t.* FROM tasks t INNER JOIN batch_tasks bt ON t.id = bt.child_task_id WHERE bt.batch_id = ?1 ORDER BY bt.seq",
        )?;
        let tasks = stmt
            .query_map(params![batch_id], Self::row_to_task)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    fn find_by_group(&self, group_id: &str) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM tasks WHERE group_id = ?1 ORDER BY created_at DESC",
        )?;
        let tasks = stmt
            .query_map(params![group_id], Self::row_to_task)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    fn batch_progress(&self, batch_id: &str) -> Result<BatchProgressAggregate, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT
                COUNT(*) as total,
                COALESCE(SUM(CASE WHEN status = ?1 THEN 1 ELSE 0 END), 0) as done,
                COALESCE(SUM(CASE WHEN status IN (?2, ?3) THEN 1 ELSE 0 END), 0) as failed,
                COALESCE(SUM(CASE WHEN status IN (?4, ?5, ?6, ?7) THEN 1 ELSE 0 END), 0) as processing
             FROM tasks WHERE batch_id = ?8",
        )?;
        let agg = stmt.query_row(
            params![
                serde_json::to_string(&TaskStatus::Done).unwrap(),
                serde_json::to_string(&TaskStatus::Failed).unwrap(),
                serde_json::to_string(&TaskStatus::Cancelled).unwrap(),
                serde_json::to_string(&TaskStatus::Processing).unwrap(),
                serde_json::to_string(&TaskStatus::Queued).unwrap(),
                serde_json::to_string(&TaskStatus::Chunking).unwrap(),
                serde_json::to_string(&TaskStatus::Merging).unwrap(),
                batch_id,
            ],
            |row| {
                Ok(BatchProgressAggregate {
                    batch_id: batch_id.to_string(),
                    total_tasks: row.get(0)?,
                    done_tasks: row.get(1)?,
                    failed_tasks: row.get(2)?,
                    processing_tasks: row.get(3)?,
                })
            },
        )?;
        Ok(agg)
    }

    fn find_all(&self) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM tasks ORDER BY created_at DESC")?;
        let tasks = stmt
            .query_map([], Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn find_standalone(&self) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM tasks WHERE group_id IS NULL ORDER BY created_at DESC"
        )?;
        let tasks = stmt
            .query_map([], Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn find_stale_processing(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let cutoff = Utc::now() - chrono::Duration::minutes(stale_minutes);
        let cutoff_str = cutoff.to_rfc3339();
        let processing_status = serde_json::to_string(&TaskStatus::Processing).unwrap();
        let chunk_pending = serde_json::to_string(&crate::domain::chunk::ChunkStatus::Pending).unwrap();
        let chunk_processing = serde_json::to_string(&crate::domain::chunk::ChunkStatus::Processing).unwrap();

        let mut stmt = conn.prepare(
            "SELECT t.* FROM tasks t
             WHERE t.status = ?1
               AND t.updated_at < ?2
               AND NOT EXISTS (
                   SELECT 1 FROM chunks c
                   WHERE c.task_id = t.id
                     AND c.status IN (?3, ?4)
               )
             ORDER BY t.updated_at ASC"
        )?;

        let tasks = stmt
            .query_map(
                params![processing_status, cutoff_str, chunk_pending, chunk_processing],
                Self::row_to_task,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn find_stale_merging(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let cutoff = Utc::now() - chrono::Duration::minutes(stale_minutes);
        let cutoff_str = cutoff.to_rfc3339();
        let merging_status = serde_json::to_string(&TaskStatus::Merging).unwrap();

        let mut stmt = conn.prepare(
            "SELECT t.* FROM tasks t
             WHERE t.status = ?1
               AND t.updated_at < ?2
             ORDER BY t.updated_at ASC"
        )?;

        let tasks = stmt
            .query_map(params![merging_status, cutoff_str], Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn find_stale_pending(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let cutoff = Utc::now() - chrono::Duration::minutes(stale_minutes);
        let cutoff_str = cutoff.to_rfc3339();
        let pending_status = serde_json::to_string(&TaskStatus::Pending).unwrap();

        let mut stmt = conn.prepare(
            "SELECT t.* FROM tasks t
             WHERE t.status = ?1
               AND t.updated_at < ?2
             ORDER BY t.updated_at ASC"
        )?;

        let tasks = stmt
            .query_map(params![pending_status, cutoff_str], Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn find_stuck_queued(&self, stale_minutes: i64) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let cutoff = Utc::now() - chrono::Duration::minutes(stale_minutes);
        let cutoff_str = cutoff.to_rfc3339();
        let queued_status = serde_json::to_string(&TaskStatus::Queued).unwrap();
        let chunking_status = serde_json::to_string(&TaskStatus::Chunking).unwrap();

        let mut stmt = conn.prepare(
            "SELECT t.* FROM tasks t
             WHERE t.status IN (?1, ?2)
               AND t.updated_at < ?3
               AND NOT EXISTS (
                   SELECT 1 FROM chunks c
                   WHERE c.task_id = t.id
               )
             ORDER BY t.updated_at ASC"
        )?;

        let tasks = stmt
            .query_map(params![queued_status, chunking_status, cutoff_str], Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn find_processing_all_done(&self) -> Result<Vec<Task>, AppError> {
        let conn = self.pool.get()?;
        let processing_status = serde_json::to_string(&TaskStatus::Processing).unwrap();

        let mut stmt = conn.prepare(
            "SELECT t.* FROM tasks t
             WHERE t.status = ?1
               AND t.total_chunks > 0
               AND (t.done_chunks + t.failed_chunks) >= t.total_chunks
             ORDER BY t.updated_at ASC"
        )?;

        let tasks = stmt
            .query_map(params![processing_status], Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    fn update_title(&self, id: &str, title: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE tasks SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        // Delete dependent rows first (foreign keys reference tasks.id)
        conn.execute("DELETE FROM chunks WHERE task_id = ?1", params![id])?;
        conn.execute("DELETE FROM batch_tasks WHERE child_task_id = ?1", params![id])?;
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn delete_all(&self) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        // Use a single transaction to ensure atomicity
        conn.execute("BEGIN", [])?;
        if let Err(e) = (|| -> Result<(), AppError> {
            conn.execute("DELETE FROM chunks", [])?;
            conn.execute("DELETE FROM batch_tasks", [])?;
            conn.execute("DELETE FROM tasks", [])?;
            Ok(())
        })() {
            conn.execute("ROLLBACK", [])?;
            return Err(e);
        }
        conn.execute("COMMIT", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task::{TaskType, CreateTaskRequest};
    use crate::infra::persistence::db::create_test_pool;
    use crate::infra::persistence::migrate::run_migrations;

    fn create_test_task() -> Task {
        Task::new(CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "test content".into(),
            content_ref: None,
            title: "Test Task".into(),
            voice: "voice_1".into(),
            model: "model_1".into(),
            style: None,
            speed: 1.0,
            provider_id: None,
            total_chars: 100,
            total_tokens: 50,
        })
    }

    #[test]
    fn test_task_insert_and_find() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteTaskRepo::new(pool);
        let task = create_test_task();
        repo.insert(&task).unwrap();
        let found = repo.find_by_id(task.id.as_str()).unwrap().unwrap();
        assert_eq!(found.id.to_string(), task.id.to_string());
        assert_eq!(found.content, task.content);
        assert_eq!(found.status, TaskStatus::Pending);
    }

    #[test]
    fn test_task_update_status() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteTaskRepo::new(pool);
        let task = create_test_task();
        repo.insert(&task).unwrap();
        repo.update_status(task.id.as_str(), &TaskStatus::Queued).unwrap();
        let found = repo.find_by_id(task.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, TaskStatus::Queued);
    }

    #[test]
    fn test_task_update_chunk_progress() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteTaskRepo::new(pool);
        let task = create_test_task();
        repo.insert(&task).unwrap();
        repo.update_chunk_progress(task.id.as_str(), 5, 3, 1, 100).unwrap();
        let found = repo.find_by_id(task.id.as_str()).unwrap().unwrap();
        assert_eq!(found.total_chunks, 5);
        assert_eq!(found.done_chunks, 3);
        assert_eq!(found.failed_chunks, 1);
    }

    #[test]
    fn test_task_set_output() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteTaskRepo::new(pool);
        let task = create_test_task();
        repo.insert(&task).unwrap();
        repo.set_output(task.id.as_str(), "/tmp/output.wav", 10.5).unwrap();
        let found = repo.find_by_id(task.id.as_str()).unwrap().unwrap();
        assert_eq!(found.output_path, Some("/tmp/output.wav".into()));
    }

    #[test]
    fn test_task_find_by_batch() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteTaskRepo::new(pool);
        let batch_id = Id::new();
        for i in 0..3 {
            let mut task = create_test_task();
            task.batch_id = Some(batch_id.clone());
            repo.insert(&task).unwrap();
            let conn = repo.pool.get().unwrap();
            conn.execute(
                "INSERT INTO batch_tasks (id, batch_id, child_task_id, seq) VALUES (?1, ?2, ?3, ?4)",
                params![Id::new().to_string(), batch_id.to_string(), task.id.to_string(), i],
            )
            .unwrap();
        }
        let tasks = repo.find_by_batch(batch_id.as_str()).unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn test_task_batch_progress() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteTaskRepo::new(pool);
        let batch_id = Id::new();
        // Insert 3 tasks
        let mut task1 = create_test_task();
        task1.batch_id = Some(batch_id.clone());
        task1.status = TaskStatus::Done;
        repo.insert(&task1).unwrap();

        let mut task2 = create_test_task();
        task2.batch_id = Some(batch_id.clone());
        task2.status = TaskStatus::Processing;
        repo.insert(&task2).unwrap();

        let mut task3 = create_test_task();
        task3.batch_id = Some(batch_id.clone());
        task3.status = TaskStatus::Pending;
        repo.insert(&task3).unwrap();

        let progress = repo.batch_progress(batch_id.as_str()).unwrap();
        assert_eq!(progress.total_tasks, 3);
        assert_eq!(progress.done_tasks, 1);
        assert_eq!(progress.failed_tasks, 0);
        assert_eq!(progress.processing_tasks, 1);
    }
}
