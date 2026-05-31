//! Chunk repository trait and SQLite implementation.

#![allow(dead_code)]

use crate::shared::id::Id;
use crate::shared::error::AppError;
use crate::domain::chunk::{Chunk, ChunkStatus};
use crate::infra::persistence::db::DbPool;
use chrono::{DateTime, Utc};
use rusqlite::params;

pub trait ChunkRepo: Send + Sync {
    fn insert(&self, chunk: &Chunk) -> Result<(), AppError>;
    fn insert_batch(&self, chunks: &[Chunk]) -> Result<(), AppError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Chunk>, AppError>;
    fn find_by_task(&self, task_id: &str) -> Result<Vec<Chunk>, AppError>;
    fn find_pending(&self, limit: i64) -> Result<Vec<Chunk>, AppError>;
    fn find_pending_prioritized(&self, limit: i64) -> Result<Vec<Chunk>, AppError>;
    fn find_oldest_pending(&self) -> Result<Option<Chunk>, AppError>;
    fn update_status(&self, id: &str, status: &ChunkStatus) -> Result<(), AppError>;
    fn update_priority(&self, id: &str, priority: i64) -> Result<(), AppError>;
    fn mark_done(&self, id: &str, audio_path: &str, duration: f64) -> Result<(), AppError>;
    fn mark_failed(&self, id: &str, error: &str) -> Result<(), AppError>;
    fn count_by_task_status(&self, task_id: &str, status: &ChunkStatus) -> Result<i64, AppError>;
    fn count_by_task_all(&self, task_id: &str) -> Result<i64, AppError>;
    fn reset_processing_to_pending(&self) -> Result<usize, AppError>;
    /// Reset chunks stuck in Processing for longer than `stale_minutes` back to Pending.
    fn reset_stale_processing_to_pending(&self, stale_minutes: i64) -> Result<usize, AppError>;
    fn delete_by_task(&self, task_id: &str) -> Result<usize, AppError>;
    /// Cancel all pending/processing chunks for a task — marks them as Failed with "Cancelled by user".
    fn cancel_pending_by_task(&self, task_id: &str) -> Result<usize, AppError>;
}

pub struct SqliteChunkRepo {
    pub pool: DbPool,
}

impl SqliteChunkRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn parse_datetime(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    fn row_to_chunk(row: &rusqlite::Row) -> rusqlite::Result<Chunk> {
        Ok(Chunk {
            id: Id::from_str(&row.get::<_, String>("id")?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            task_id: Id::from_str(&row.get::<_, String>("task_id")?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            seq: row.get("seq")?,
            text: row.get("text")?,
            status: serde_json::from_str(&row.get::<_, String>("status")?).unwrap(),
            retry_count: row.get("retry_count")?,
            max_retries: row.get("max_retries")?,
            priority: row.get::<_, i64>("priority")?,
            audio_path: row.get("audio_path")?,
            duration: row
                .get::<_, Option<i64>>("duration_ms")?
                .map(|d| d as f64 / 1000.0),
            created_at: Self::parse_datetime(&row.get::<_, String>("created_at")?),
            updated_at: Self::parse_datetime(&row.get::<_, String>("updated_at")?),
            completed_at: None, // chunks table has no completed_at column
        })
    }
}

impl ChunkRepo for SqliteChunkRepo {
    fn insert(&self, chunk: &Chunk) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO chunks (id, task_id, seq, status, text, char_count, token_count,
             retry_count, max_retries, priority, audio_path, duration_ms, error_message,
             created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,NULL,?12,?13)",
            params![
                chunk.id.to_string(),
                chunk.task_id.to_string(),
                chunk.seq,
                serde_json::to_string(&chunk.status).unwrap(),
                chunk.text,
                chunk.text.len() as i64,
                0i64,
                chunk.retry_count,
                chunk.max_retries,
                chunk.priority,
                chunk.audio_path,
                chunk.created_at.to_rfc3339(),
                chunk.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    fn insert_batch(&self, chunks: &[Chunk]) -> Result<(), AppError> {
        if chunks.is_empty() {
            return Ok(());
        }
        let conn = self.pool.get()?;
        let tx = conn.unchecked_transaction()?;
        for chunk in chunks {
            tx.execute(
                "INSERT INTO chunks (id, task_id, seq, status, text, char_count, token_count,
                 retry_count, max_retries, priority, audio_path, duration_ms, error_message,
                 created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,NULL,?12,?13)",
                params![
                    chunk.id.to_string(),
                    chunk.task_id.to_string(),
                    chunk.seq,
                    serde_json::to_string(&chunk.status).unwrap(),
                    chunk.text,
                    chunk.text.len() as i64,
                    0i64,
                    chunk.retry_count,
                    chunk.max_retries,
                    chunk.priority,
                    chunk.audio_path,
                    chunk.created_at.to_rfc3339(),
                    chunk.updated_at.to_rfc3339(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn find_by_id(&self, id: &str) -> Result<Option<Chunk>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM chunks WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], Self::row_to_chunk)?;
        match rows.next() {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            _ => Ok(None),
        }
    }

    fn find_by_task(&self, task_id: &str) -> Result<Vec<Chunk>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM chunks WHERE task_id = ?1 ORDER BY seq ASC",
        )?;
        let chunks = stmt
            .query_map(params![task_id], Self::row_to_chunk)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(chunks)
    }

    fn find_pending(&self, limit: i64) -> Result<Vec<Chunk>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM chunks WHERE status = ?1 ORDER BY created_at ASC LIMIT ?2",
        )?;
        let chunks = stmt
            .query_map(
                params![serde_json::to_string(&ChunkStatus::Pending).unwrap(), limit],
                Self::row_to_chunk,
            )?
            .filter_map(|r| r.ok())
            .collect();
        Ok(chunks)
    }

    fn find_pending_prioritized(&self, limit: i64) -> Result<Vec<Chunk>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM chunks WHERE status = ?1 ORDER BY priority DESC, created_at ASC LIMIT ?2",
        )?;
        let chunks = stmt
            .query_map(
                params![serde_json::to_string(&ChunkStatus::Pending).unwrap(), limit],
                Self::row_to_chunk,
            )?
            .filter_map(|r| r.ok())
            .collect();
        Ok(chunks)
    }

    fn find_oldest_pending(&self) -> Result<Option<Chunk>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM chunks WHERE status = ?1 ORDER BY created_at ASC LIMIT 1",
        )?;
        let mut rows = stmt
            .query_map(
                params![serde_json::to_string(&ChunkStatus::Pending).unwrap()],
                Self::row_to_chunk,
            )?;
        match rows.next() {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            _ => Ok(None),
        }
    }

    fn update_status(&self, id: &str, status: &ChunkStatus) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![serde_json::to_string(status).unwrap(), Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn update_priority(&self, id: &str, priority: i64) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            params![priority, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn mark_done(&self, id: &str, audio_path: &str, duration: f64) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET status = ?1, audio_path = ?2, duration_ms = ?3, updated_at = ?4 WHERE id = ?5",
            params![
                serde_json::to_string(&ChunkStatus::Done).unwrap(),
                audio_path,
                (duration * 1000.0) as i64,
                Utc::now().to_rfc3339(),
                id,
            ],
        )?;
        Ok(())
    }

    fn mark_failed(&self, id: &str, error: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET status = ?1, error_message = ?2, retry_count = retry_count + 1, updated_at = ?3 WHERE id = ?4",
            params![
                serde_json::to_string(&ChunkStatus::Failed).unwrap(),
                error,
                Utc::now().to_rfc3339(),
                id,
            ],
        )?;
        Ok(())
    }

    fn count_by_task_status(&self, task_id: &str, status: &ChunkStatus) -> Result<i64, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM chunks WHERE task_id = ?1 AND status = ?2",
        )?;
        let count = stmt.query_row(
            params![task_id, serde_json::to_string(status).unwrap()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count)
    }

    fn count_by_task_all(&self, task_id: &str) -> Result<i64, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM chunks WHERE task_id = ?1")?;
        let count = stmt.query_row(params![task_id], |row| row.get::<_, i64>(0))?;
        Ok(count)
    }

    fn reset_processing_to_pending(&self) -> Result<usize, AppError> {
        let conn = self.pool.get()?;
        let affected = conn.execute(
            "UPDATE chunks SET status = ?1, updated_at = ?2 WHERE status = ?3",
            params![
                serde_json::to_string(&ChunkStatus::Pending).unwrap(),
                Utc::now().to_rfc3339(),
                serde_json::to_string(&ChunkStatus::Processing).unwrap(),
            ],
        )?;
        Ok(affected)
    }

    fn reset_stale_processing_to_pending(&self, stale_minutes: i64) -> Result<usize, AppError> {
        let conn = self.pool.get()?;
        let cutoff = (Utc::now() - chrono::Duration::minutes(stale_minutes)).to_rfc3339();
        let affected = conn.execute(
            "UPDATE chunks SET status = ?1, updated_at = ?2 WHERE status = ?3 AND updated_at < ?4",
            params![
                serde_json::to_string(&ChunkStatus::Pending).unwrap(),
                Utc::now().to_rfc3339(),
                serde_json::to_string(&ChunkStatus::Processing).unwrap(),
                cutoff,
            ],
        )?;
        Ok(affected)
    }

    fn cancel_pending_by_task(&self, task_id: &str) -> Result<usize, AppError> {
        let conn = self.pool.get()?;
        let pending_str = serde_json::to_string(&ChunkStatus::Pending).unwrap();
        let processing_str = serde_json::to_string(&ChunkStatus::Processing).unwrap();
        let failed_str = serde_json::to_string(&ChunkStatus::Failed).unwrap();
        let affected = conn.execute(
            "UPDATE chunks SET status = ?1, error_message = ?2, updated_at = ?3 \
             WHERE task_id = ?4 AND status IN (?5, ?6)",
            params![
                failed_str,
                "Cancelled by user",
                Utc::now().to_rfc3339(),
                task_id,
                pending_str,
                processing_str,
            ],
        )?;
        Ok(affected)
    }

    fn delete_by_task(&self, task_id: &str) -> Result<usize, AppError> {
        let conn = self.pool.get()?;
        let affected = conn.execute("DELETE FROM chunks WHERE task_id = ?1", params![task_id])?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
use crate::infra::persistence::db::create_test_pool;
use crate::infra::persistence::migrate::run_migrations;
use rusqlite::Connection;

/// Insert a minimal parent task so FK constraints don't fail.
fn insert_test_task(conn: &Connection, task_id: &Id) {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (id, task_type, status, content, title, voice, model, speed,
         priority, total_chars, total_tokens, total_chunks, done_chunks, failed_chunks,
         output_path, output_duration, created_at, updated_at)
         VALUES (?1,'\"Single\"','\"Pending\"','test','test','v','m',1.0,0,0,0,0,0,0,NULL,0,?2,?3)",
        params![task_id.to_string(), now, now],
    ).unwrap();
}

fn create_test_chunk(task_id: &Id) -> Chunk {
        let mut chunk = Chunk::new(task_id.clone(), 1, "test chunk content".into());
        chunk.priority = 5;
        chunk
    }

    #[test]
    fn test_chunk_insert_and_find() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let chunk = create_test_chunk(&task_id);
        repo.insert(&chunk).unwrap();
        let found = repo.find_by_id(chunk.id.as_str()).unwrap().unwrap();
        assert_eq!(found.id.to_string(), chunk.id.to_string());
        assert_eq!(found.text, chunk.text);
        assert_eq!(found.priority, 5);
        assert_eq!(found.status, ChunkStatus::Pending);
    }

    #[test]
    fn test_chunk_insert_batch() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let chunks: Vec<Chunk> = (0..3)
            .map(|i| {
                let mut c = Chunk::new(task_id.clone(), i, format!("chunk {}", i));
                c.priority = i as i64;
                c
            })
            .collect();
        repo.insert_batch(&chunks).unwrap();
        let found = repo.find_by_task(task_id.as_str()).unwrap();
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn test_chunk_find_by_task() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        for i in 0..3 {
            let chunk = Chunk::new(task_id.clone(), i, format!("chunk {}", i));
            repo.insert(&chunk).unwrap();
        }
        let chunks = repo.find_by_task(task_id.as_str()).unwrap();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].seq, 0);
        assert_eq!(chunks[2].seq, 2);
    }

    #[test]
    fn test_chunk_find_pending() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let chunk1 = Chunk::new(task_id.clone(), 0, "first".into());
        let mut chunk2 = Chunk::new(task_id.clone(), 1, "second".into());
        // chunk2 needs mut for transition_to
        chunk2
            .transition_to(ChunkStatus::Queued)
            .unwrap();
        repo.insert(&chunk1).unwrap();
        repo.insert(&chunk2).unwrap();
        let pending = repo.find_pending(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].text, "first");
    }

    #[test]
    fn test_chunk_find_pending_prioritized() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let mut c1 = Chunk::new(task_id.clone(), 0, "low".into());
        c1.priority = 1;
        let mut c2 = Chunk::new(task_id.clone(), 1, "high".into());
        c2.priority = 10;
        repo.insert(&c1).unwrap();
        repo.insert(&c2).unwrap();
        let pending = repo.find_pending_prioritized(10).unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].text, "high"); // priority DESC
    }

    #[test]
    fn test_chunk_find_oldest_pending() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let c1 = Chunk::new(task_id.clone(), 0, "oldest".into());
        repo.insert(&c1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let c2 = Chunk::new(task_id.clone(), 1, "newer".into());
        repo.insert(&c2).unwrap();
        let oldest = repo.find_oldest_pending().unwrap().unwrap();
        assert_eq!(oldest.text, "oldest");
    }

    #[test]
    fn test_chunk_mark_done() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let chunk = Chunk::new(task_id.clone(), 0, "test".into());
        repo.insert(&chunk).unwrap();
        repo.mark_done(chunk.id.as_str(), "/tmp/test.wav", 3.5)
            .unwrap();
        let found = repo.find_by_id(chunk.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, ChunkStatus::Done);
        assert_eq!(found.audio_path, Some("/tmp/test.wav".into()));
    }

    #[test]
    fn test_chunk_mark_failed() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let chunk = Chunk::new(task_id.clone(), 0, "test".into());
        repo.insert(&chunk).unwrap();
        repo.mark_failed(chunk.id.as_str(), "API error").unwrap();
        let found = repo.find_by_id(chunk.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, ChunkStatus::Failed);
        assert_eq!(found.retry_count, 1);
    }

    #[test]
    fn test_chunk_update_priority() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let chunk = Chunk::new(task_id.clone(), 0, "test".into());
        repo.insert(&chunk).unwrap();
        repo.update_priority(chunk.id.as_str(), 99).unwrap();
        let found = repo.find_by_id(chunk.id.as_str()).unwrap().unwrap();
        assert_eq!(found.priority, 99);
    }

    #[test]
    fn test_chunk_count_by_task_status() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        for i in 0..5 {
            let chunk = Chunk::new(task_id.clone(), i, format!("c{}", i));
            repo.insert(&chunk).unwrap();
        }
        let count = repo.count_by_task_status(task_id.as_str(), &ChunkStatus::Pending).unwrap();
        assert_eq!(count, 5);
        let all = repo.count_by_task_all(task_id.as_str()).unwrap();
        assert_eq!(all, 5);
    }

    #[test]
    fn test_chunk_reset_processing_to_pending() {
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        let task_id = Id::new();
        insert_test_task(&conn, &task_id);
        let repo = SqliteChunkRepo::new(pool);
        let mut chunk = Chunk::new(task_id.clone(), 0, "test".into());
        chunk
            .transition_to(ChunkStatus::Queued)
            .unwrap();
        chunk
            .transition_to(ChunkStatus::Processing)
            .unwrap();
        repo.insert(&chunk).unwrap();
        let reset = repo.reset_processing_to_pending().unwrap();
        assert_eq!(reset, 1);
        let found = repo.find_by_id(chunk.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, ChunkStatus::Pending);
    }
}
