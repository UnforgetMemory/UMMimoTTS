//! Repositories over schema v4.
//!
//! All list queries push pagination/filtering into SQL (ADR: no full-table
//! loads, no in-memory paging — v3 bug fixed). Chunk claiming uses an
//! optimistic atomic UPDATE.

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use mimotts_core::domain::{
    Chunk, Id, Session, Task, TaskStatus,
};

use crate::error::EngineError;
use crate::storage::DbPool;

// ── row DTOs (wire/DB shape) ─────────────────────────────────────────────

// api_key_sealed must never serialize into responses — internal DTO only.
#[derive(Debug, Clone)]
pub struct ProviderRow {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub kind: String,
    pub api_key_sealed: String,
    pub budget_group: String,
    pub is_configured: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub total_tasks: i32,
    pub done_tasks: i32,
    pub failed_tasks: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub session_id: Option<String>,
    pub title: String,
    /// Full text is detail-only; list rows leave it empty and the field is
    /// omitted from JSON so list endpoints never ship book-sized payloads.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub status: String,
    pub total_chars: i64,
    pub total_tokens: i64,
    pub total_chunks: i32,
    pub done_chunks: i32,
    pub failed_chunks: i32,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub has_audio: bool,
    pub created_at: String,
    pub completed_at: Option<String>,
    #[serde(default, skip)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRow {
    pub id: String,
    pub task_id: String,
    pub seq: i32,
    pub text: String,
    pub token_estimate: i64,
    pub status: String,
    pub retry_count: i32,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

/// v4 contract: statuses are BARE lowercase strings — never JSON-quoted.
fn task_status_str(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Queued => "queued",
        TaskStatus::Synthesizing => "synthesizing",
        TaskStatus::Merging => "merging",
        TaskStatus::Done => "done",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Minimal worker-facing task metadata.
#[derive(Debug, Clone)]
pub struct TaskMeta {
    pub status: String,
    pub session_id: Option<String>,
    pub provider_id: Option<String>,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub content: String,
    pub title: String,
    pub priority: i64,
}

// ── storage façade ───────────────────────────────────────────────────────

pub struct Storage {
    pub pool: DbPool,
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

impl Storage {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    // ── providers ────────────────────────────────────────────────────────

    pub fn providers(&self) -> Result<Vec<ProviderRow>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, kind, api_key_sealed, budget_group, is_configured, is_default
             FROM providers ORDER BY CASE WHEN is_default=1 THEN 0 ELSE 1 END, name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProviderRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    base_url: r.get(2)?,
                    kind: r.get(3)?,
                    api_key_sealed: r.get(4)?,
                    budget_group: r.get(5)?,
                    is_configured: r.get::<_, i64>(6)? != 0,
                    is_default: r.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn provider(&self, id: &str) -> Result<Option<ProviderRow>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, kind, api_key_sealed, budget_group, is_configured, is_default
             FROM providers WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(ProviderRow {
                id: r.get(0)?,
                name: r.get(1)?,
                base_url: r.get(2)?,
                kind: r.get(3)?,
                api_key_sealed: r.get(4)?,
                budget_group: r.get(5)?,
                is_configured: r.get::<_, i64>(6)? != 0,
                is_default: r.get::<_, i64>(7)? != 0,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn set_provider_key(
        &self,
        id: &str,
        api_key_sealed: &str,
        is_configured: bool,
    ) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        let n = conn.execute(
            "UPDATE providers SET api_key_sealed=?1, is_configured=?2, updated_at=?3 WHERE id=?4",
            params![api_key_sealed, is_configured as i64, now(), id],
        )?;
        if n == 0 {
            return Err(EngineError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }

    pub fn set_default_provider(&self, id: &str) -> Result<(), EngineError> {
        let mut conn = self.pool.get()?;
        // Single transaction: a crash between the two UPDATEs must not leave
        // the table with zero default providers.
        let tx = conn.transaction()?;
        tx.execute("UPDATE providers SET is_default=0", [])?;
        let n = tx.execute(
            "UPDATE providers SET is_default=1, updated_at=?1 WHERE id=?2",
            params![now(), id],
        )?;
        if n == 0 {
            return Err(EngineError::NotFound(format!("provider {id}")));
        }
        tx.commit()?;
        Ok(())
    }

    /// Edit provider metadata (custom upstreams, budget grouping).
    pub fn edit_provider(
        &self,
        id: &str,
        name: Option<&str>,
        base_url: Option<&str>,
        budget_group: Option<&str>,
    ) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        let mut sets: Vec<&str> = Vec::new();
        let mut vals: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(n) = name {
            sets.push("name = ?");
            vals.push(Box::new(n.to_string()));
        }
        if let Some(b) = base_url {
            sets.push("base_url = ?");
            vals.push(Box::new(b.trim_end_matches('/').to_string()));
        }
        if let Some(g) = budget_group {
            sets.push("budget_group = ?");
            vals.push(Box::new(g.to_string()));
        }
        if sets.is_empty() {
            return Ok(());
        }
        sets.push("updated_at = ?");
        vals.push(Box::new(now()));
        let sql = format!("UPDATE providers SET {} WHERE id = ?", sets.join(", "));
        vals.push(Box::new(id.to_string()));
        let n = conn.execute(
            &sql,
            vals.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
        )?;
        if n == 0 {
            return Err(EngineError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }

    // ── sessions ─────────────────────────────────────────────────────────

    pub fn create_session(&self, name: &str) -> Result<Session, EngineError> {
        let s = Session::new(name.to_string());
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO sessions (id, name, status, total_tasks, done_tasks, failed_tasks, created_at, updated_at)
             VALUES (?1, ?2, 'active', 0, 0, 0, ?3, ?3)",
            params![s.id.to_string(), s.name, now()],
        )?;
        Ok(s)
    }

    pub fn session(&self, id: &str) -> Result<Option<SessionRow>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, status, total_tasks, done_tasks, failed_tasks, created_at, completed_at
             FROM sessions WHERE id=?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_session)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn sessions(&self, page: i64, page_size: i64) -> Result<(Vec<SessionRow>, i64), EngineError> {
        let conn = self.pool.get()?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        let mut stmt = conn.prepare(
            "SELECT id, name, status, total_tasks, done_tasks, failed_tasks, created_at, completed_at
             FROM sessions ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(params![page_size, page * page_size], row_to_session)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((rows, total))
    }

    pub fn delete_session(&self, id: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        // output files are removed by the engine layer before cascade
        let n = conn.execute("DELETE FROM sessions WHERE id=?1", params![id])?;
        if n == 0 {
            return Err(EngineError::NotFound(format!("session {id}")));
        }
        Ok(())
    }

    pub fn update_session_progress(
        &self,
        id: &str,
        done_delta: i32,
        failed_delta: i32,
    ) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE sessions SET
               done_tasks   = done_tasks + ?2,
               failed_tasks = failed_tasks + ?3,
               total_tasks  = total_tasks + ?4,
               status = CASE
                 WHEN (done_tasks + ?2) + (failed_tasks + ?3) >= total_tasks + ?4
                      AND total_tasks + ?4 > 0 THEN
                   CASE WHEN (failed_tasks + ?3) > 0 THEN 'failed' ELSE 'completed' END
                 ELSE status END,
               completed_at = CASE
                 WHEN (done_tasks + ?2) + (failed_tasks + ?3) >= total_tasks + ?4
                      AND total_tasks + ?4 > 0 THEN ?5
                 ELSE completed_at END,
               updated_at = ?5
             WHERE id = ?1",
            params![id, done_delta, failed_delta, 0i32, now()],
        )?;
        Ok(())
    }

    pub fn add_session_total(&self, id: &str, delta: i32) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE sessions SET total_tasks = total_tasks + ?2, updated_at = ?3 WHERE id = ?1",
            params![id, delta, now()],
        )?;
        Ok(())
    }

    // ── tasks ────────────────────────────────────────────────────────────

    pub fn insert_task(&self, task: &Task) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO tasks (id, session_id, title, content, voice, model, style, status, priority,
                                total_chars, total_tokens, total_chunks, done_chunks, failed_chunks,
                                created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'pending',?8,?9,?10,0,0,0,?11,?11)",
            params![
                task.id.to_string(),
                task.session_id.as_ref().map(|s| s.to_string()),
                task.title,
                task.content,
                task.voice,
                task.model,
                task.style,
                task.priority,
                task.total_chars,
                task.total_tokens,
                now(),
            ],
        )?;
        Ok(())
    }

    pub fn task(&self, id: &str) -> Result<Option<(Task, Vec<ChunkRow>)>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, title, content, voice, model, style, status, total_chars, total_tokens,
                    total_chunks, done_chunks, failed_chunks, duration_ms, error, output_path,
                    CASE WHEN output_path IS NOT NULL AND output_path != '' THEN 1 ELSE 0 END,
                    created_at, completed_at, updated_at
             FROM tasks WHERE id=?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(TaskRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                title: r.get(2)?,
                content: r.get(3)?,
                voice: r.get(4)?,
                model: r.get(5)?,
                style: r.get(6)?,
                status: r.get(7)?,
                total_chars: r.get(8)?,
                total_tokens: r.get(9)?,
                total_chunks: r.get(10)?,
                done_chunks: r.get(11)?,
                failed_chunks: r.get(12)?,
                duration_ms: r.get(13)?,
                error: r.get(14)?,
                has_audio: r.get::<_, i64>(16)? != 0,
                created_at: r.get(17)?,
                completed_at: r.get(18)?,
                updated_at: r.get(19)?,
            })
        })?;
        let row = match rows.next() {
            Some(r) => r?,
            None => return Ok(None),
        };
        let task = task_from_row(row);
        let chunks = self.chunks(id)?;
        Ok(Some((task, chunks)))
    }

    pub fn tasks(
        &self,
        page: i64,
        page_size: i64,
        status: Option<&str>,
        session_id: Option<&str>,
        search: Option<&str>,
    ) -> Result<(Vec<TaskRow>, i64), EngineError> {
        let conn = self.pool.get()?;
        let mut where_sql = String::from("WHERE 1=1");
        let mut bind: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(s) = status {
            where_sql.push_str(" AND status = ?");
            bind.push(Box::new(s.to_string()));
        }
        if let Some(sid) = session_id {
            where_sql.push_str(" AND session_id = ?");
            bind.push(Box::new(sid.to_string()));
        }
        if let Some(q) = search {
            // Escape LIKE wildcards so user input stays literal (parameterized
            // already — this is about `%`/`_` matching everything).
            let escaped = q
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            where_sql.push_str(" AND title LIKE ? ESCAPE '\\'");
            bind.push(Box::new(format!("%{escaped}%")));
        }
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM tasks {where_sql}"),
            bind.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
            |r| r.get(0),
        )?;
        // No full `content` in list paths: pages of book-length tasks would
        // load megabytes just to render a row list.
        let query = format!(
            "SELECT id, session_id, title, voice, model, style, status, total_chars, total_tokens,
                    total_chunks, done_chunks, failed_chunks, duration_ms, error,
                    CASE WHEN output_path IS NOT NULL AND output_path != '' THEN 1 ELSE 0 END,
                    created_at, completed_at
             FROM tasks {where_sql}
             ORDER BY created_at DESC LIMIT ? OFFSET ?"
        );
        let mut params2: Vec<Box<dyn rusqlite::types::ToSql>> = bind;
        params2.push(Box::new(page_size));
        params2.push(Box::new(page.max(0).saturating_mul(page_size)));
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt
            .query_map(
                params2.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
                |r| {
                    Ok(TaskRow {
                        id: r.get(0)?,
                        session_id: r.get(1)?,
                        title: r.get(2)?,
                        content: String::new(),
                        voice: r.get(3)?,
                        model: r.get(4)?,
                        style: r.get(5)?,
                        status: r.get(6)?,
                        total_chars: r.get(7)?,
                        total_tokens: r.get(8)?,
                        total_chunks: r.get(9)?,
                        done_chunks: r.get(10)?,
                        failed_chunks: r.get(11)?,
                        duration_ms: r.get(12)?,
                        error: r.get(13)?,
                        has_audio: r.get::<_, i64>(14)? != 0,
                        created_at: r.get(15)?,
                        completed_at: r.get(16)?,
                        updated_at: String::new(),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((rows, total))
    }

    pub fn update_task_status(&self, id: &str, status: &TaskStatus) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        let is_terminal = status.is_terminal();
        conn.execute(
            "UPDATE tasks SET status=?2, updated_at=?3,
               completed_at = CASE WHEN ?4 THEN COALESCE(completed_at, ?3) ELSE completed_at END
             WHERE id=?1",
            params![id, task_status_str(status), now(), is_terminal],
        )?;
        Ok(())
    }

    pub fn set_task_output(&self, id: &str, output_path: &str, duration_ms: i64) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE tasks SET output_path=?2, duration_ms=?3, updated_at=?4 WHERE id=?1",
            params![id, output_path, duration_ms, now()],
        )?;
        Ok(())
    }

    pub fn set_task_error(&self, id: &str, error: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE tasks SET error=?2, updated_at=?3 WHERE id=?1",
            params![id, error, now()],
        )?;
        Ok(())
    }

    pub fn task_output_path(&self, id: &str) -> Result<Option<String>, EngineError> {
        let conn = self.pool.get()?;
        Ok(conn.query_row(
            "SELECT output_path FROM tasks WHERE id=?1",
            params![id],
            |r| r.get(0),
        )?)
    }

    pub fn delete_task(&self, id: &str) -> Result<Option<String>, EngineError> {
        let conn = self.pool.get()?;
        let output: Option<String> =
            conn.query_row("SELECT output_path FROM tasks WHERE id=?1", params![id], |r| {
                r.get(0)
            })?;
        conn.execute("DELETE FROM tasks WHERE id=?1", params![id])?;
        Ok(output)
    }

    pub fn mark_task_cancelled(&self, id: &str) -> Result<(), EngineError> {
        self.update_task_status(id, &TaskStatus::Cancelled)
    }

    // ── chunks ───────────────────────────────────────────────────────────

    pub fn insert_chunks(&self, chunks: &[Chunk]) -> Result<(), EngineError> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chunks (id, task_id, seq, text, token_estimate, status, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,'pending',?6,?6)",
            )?;
            for c in chunks {
                stmt.execute(params![
                    c.id.to_string(),
                    c.task_id.to_string(),
                    c.seq,
                    c.text,
                    c.token_estimate,
                    now(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn chunks(&self, task_id: &str) -> Result<Vec<ChunkRow>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, seq, text, token_estimate, status, retry_count, duration_ms, error
             FROM chunks WHERE task_id=?1 ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![task_id], |r| {
                Ok(ChunkRow {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    seq: r.get(2)?,
                    text: r.get(3)?,
                    token_estimate: r.get(4)?,
                    status: r.get(5)?,
                    retry_count: r.get(6)?,
                    duration_ms: r.get(7)?,
                    error: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Optimistic claim: atomically Pending → InFlight.
    pub fn claim_chunk(&self, chunk_id: &str) -> Result<bool, EngineError> {
        let conn = self.pool.get()?;
        let n = conn.execute(
            "UPDATE chunks SET status='inflight', updated_at=?2 WHERE id=?1 AND status='pending'",
            params![chunk_id, now()],
        )?;
        Ok(n == 1)
    }

    /// Guarded finish: only an `inflight` chunk may become `done`.
    /// `range` addresses the audio inside the shared raw stream
    /// (`audio_path`); legacy chunks without a range own the whole file.
    /// Returns false when the chunk was cancelled mid-flight (no overwrite).
    pub fn finish_chunk(
        &self,
        chunk_id: &str,
        audio_path: &str,
        range: Option<(i64, i64)>,
        duration_ms: i64,
    ) -> Result<bool, EngineError> {
        let conn = self.pool.get()?;
        let n = conn.execute(
            "UPDATE chunks SET status='done', audio_path=?2, audio_offset=?3, audio_len=?4,
               duration_ms=?5, error=NULL, updated_at=?6, completed_at=?6
             WHERE id=?1 AND status='inflight'",
            params![
                chunk_id,
                audio_path,
                range.as_ref().map(|r| r.0),
                range.as_ref().map(|r| r.1),
                duration_ms,
                now()
            ],
        )?;
        Ok(n == 1)
    }

    /// Guarded fail: never overwrite a `done` chunk (cancel race guard).
    pub fn fail_chunk(&self, chunk_id: &str, error: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET status='failed', retry_count=retry_count+1, error=?2, updated_at=?3
             WHERE id=?1 AND status != 'done'",
            params![chunk_id, error, now()],
        )?;
        Ok(())
    }

    /// Atomic merge claim (umreview C2): exactly one caller may transition
    /// the task into `merging`. The loser skips the merge entirely.
    pub fn claim_merge(&self, task_id: &str) -> Result<bool, EngineError> {
        let conn = self.pool.get()?;
        let n = conn.execute(
            "UPDATE tasks SET status='merging', updated_at=?2
             WHERE id=?1 AND status IN ('synthesizing','queued')",
            params![task_id, now()],
        )?;
        Ok(n == 1)
    }

    /// Atomic terminal claim for the all-failed path (C2 companion): exactly
    /// one resolver may transition the task into `failed`, so concurrent
    /// final-chunk failures neither double-emit TaskFailed nor double-decrement
    /// the session counter.
    pub fn claim_task_failed(&self, task_id: &str) -> Result<bool, EngineError> {
        let conn = self.pool.get()?;
        let n = conn.execute(
            "UPDATE tasks SET status='failed', updated_at=?2,
                completed_at=COALESCE(completed_at, ?2)
             WHERE id=?1 AND status IN ('synthesizing','queued')",
            params![task_id, now()],
        )?;
        Ok(n == 1)
    }

    /// Recovery: reset tasks stuck in `merging` (merge never ran / worker died
    /// between claim and spawn) back to `synthesizing` so the resolve path
    /// re-runs. Returns the affected task ids.
    pub fn reset_stale_merging(&self, stale_secs: i64) -> Result<Vec<String>, EngineError> {
        let conn = self.pool.get()?;
        let cutoff = (Utc::now() - chrono::Duration::seconds(stale_secs)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT id FROM tasks WHERE status='merging' AND updated_at < ?1",
        )?;
        let ids: Vec<String> = stmt
            .query_map(params![cutoff], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        conn.execute(
            "UPDATE tasks SET status='synthesizing', updated_at=?2
             WHERE status='merging' AND updated_at < ?1",
            params![cutoff, now()],
        )?;
        Ok(ids)
    }

    /// Re-chunk reset: delete ALL chunks of a task (any status) and return
    /// their audio files for on-disk reclamation. In-flight chunks become
    /// no-ops (their guarded `finish_chunk` affects 0 rows).
    pub fn reset_all_chunks(&self, task_id: &str) -> Result<Vec<String>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT audio_path FROM chunks
             WHERE task_id=?1 AND audio_path IS NOT NULL AND audio_path != ''",
        )?;
        let files = stmt
            .query_map(params![task_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        conn.execute("DELETE FROM chunks WHERE task_id=?1", params![task_id])?;
        Ok(files)
    }

    pub fn max_chunk_seq(&self, task_id: &str) -> Result<i32, EngineError> {
        let conn = self.pool.get()?;
        let n: i32 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM chunks WHERE task_id=?1",
            params![task_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Highest DONE seq (live assembler resumes from here after restart).
    pub fn max_done_seq(&self, task_id: &str) -> Result<i32, EngineError> {
        let conn = self.pool.get()?;
        let n: i32 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM chunks WHERE task_id=?1 AND status='done'",
            params![task_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn reset_failed_chunks(&self, task_id: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET status='pending', error=NULL, updated_at=?2 WHERE task_id=?1 AND status='failed'",
            params![task_id, now()],
        )?;
        Ok(())
    }

    pub fn cancel_pending_chunks(&self, task_id: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE chunks SET status='failed', error='cancelled', updated_at=?2
             WHERE task_id=?1 AND status IN ('pending','inflight')",
            params![task_id, now()],
        )?;
        Ok(())
    }

    /// Done chunks in seq order with their audio location:
    /// `(seq, path, offset, len, duration)`. `offset/len` are None for legacy
    /// rows that own a whole per-chunk file (pre-stream layout).
    pub fn chunk_audio_ranges(
        &self,
        task_id: &str,
    ) -> Result<Vec<(i32, String, Option<i64>, Option<i64>, Option<i64>)>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT seq, audio_path, audio_offset, audio_len, duration_ms FROM chunks
             WHERE task_id=?1 AND status='done' AND audio_path IS NOT NULL ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![task_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Every DISTINCT audio file of a task (shared raw stream + any legacy
    /// per-chunk files) — for on-disk reclamation.
    pub fn task_audio_files(&self, task_id: &str) -> Result<Vec<String>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT audio_path FROM chunks
             WHERE task_id=?1 AND audio_path IS NOT NULL AND audio_path != ''",
        )?;
        let rows = stmt
            .query_map(params![task_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn chunk_stats(&self, task_id: &str) -> Result<(i64, i64, i64, i64), EngineError> {
        let conn = self.pool.get()?;
        conn.query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN status='done' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status IN ('pending','inflight') THEN 1 ELSE 0 END)
             FROM chunks WHERE task_id=?1",
            params![task_id],
            |r| {
                let total: i64 = r.get(0)?;
                let done: Option<i64> = r.get(1)?;
                let failed: Option<i64> = r.get(2)?;
                let active: Option<i64> = r.get(3)?;
                Ok((total, done.unwrap_or(0), failed.unwrap_or(0), active.unwrap_or(0)))
            },
        )
        .map_err(Into::into)
    }

    pub fn update_task_progress(
        &self,
        task_id: &str,
        total: i32,
        done: i32,
        failed: i32,
    ) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE tasks SET total_chunks=?2, done_chunks=?3, failed_chunks=?4, updated_at=?5 WHERE id=?1",
            params![task_id, total, done, failed, now()],
        )?;
        Ok(())
    }

    /// Minimal worker-facing task metadata (one row, no content for list paths).
    pub fn task_meta(&self, id: &str) -> Result<Option<TaskMeta>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT status, session_id, provider_id, voice, model, style, content, title, priority
             FROM tasks WHERE id=?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(TaskMeta {
                status: r.get(0)?,
                session_id: r.get(1)?,
                provider_id: r.get(2)?,
                voice: r.get(3)?,
                model: r.get(4)?,
                style: r.get(5)?,
                content: r.get(6)?,
                title: r.get(7)?,
                priority: r.get(8)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn chunk_row(&self, id: &str) -> Result<Option<ChunkRow>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, seq, text, token_estimate, status, retry_count, duration_ms, error
             FROM chunks WHERE id=?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(ChunkRow {
                id: r.get(0)?,
                task_id: r.get(1)?,
                seq: r.get(2)?,
                text: r.get(3)?,
                token_estimate: r.get(4)?,
                status: r.get(5)?,
                retry_count: r.get(6)?,
                duration_ms: r.get(7)?,
                error: r.get(8)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn delete_chunks(&self, task_id: &str) -> Result<usize, EngineError> {
        let conn = self.pool.get()?;
        let n = conn.execute("DELETE FROM chunks WHERE task_id=?1", params![task_id])?;
        Ok(n)
    }

    pub fn session_task_ids(&self, session_id: &str) -> Result<Vec<String>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT id FROM tasks WHERE session_id=?1")?;
        let rows = stmt
            .query_map(params![session_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn session_outputs(&self, session_id: &str) -> Result<Vec<(String, String)>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT title, output_path FROM tasks
             WHERE session_id=?1 AND status='done' AND output_path IS NOT NULL AND output_path != ''
             ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map(params![session_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Recovery: reset inflight chunks stale for > `stale_secs`.
    pub fn reset_stale_inflight(&self, stale_secs: i64) -> Result<usize, EngineError> {
        let conn = self.pool.get()?;
        let cutoff = (Utc::now() - chrono::Duration::seconds(stale_secs)).to_rfc3339();
        let n = conn.execute(
            "UPDATE chunks SET status='pending', updated_at=?2 WHERE status='inflight' AND updated_at < ?1",
            params![cutoff, now()],
        )?;
        Ok(n)
    }

    /// Recovery: pending chunk ids to re-seed the in-memory queue (bounded).
    /// Returns (chunk_id, task_id, task_priority) — priority is preserved.
    pub fn pending_chunk_ids(&self, limit: i64) -> Result<Vec<(String, String, i64)>, EngineError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.task_id, COALESCE(t.priority, 0) FROM chunks c
             LEFT JOIN tasks t ON t.id = c.task_id
             WHERE c.status='pending'
             ORDER BY t.priority DESC, c.created_at ASC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // ── tokens / config ──────────────────────────────────────────────────

    pub fn store_token_hash(&self, token_hash: &str, label: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO api_tokens (id, token_hash, label, created_at) VALUES (?1,?2,?3,?4)",
            params![mimotts_core::domain::Id::new().to_string(), token_hash, label, now()],
        )?;
        Ok(())
    }

    pub fn token_exists(&self, token_hash: &str) -> Result<bool, EngineError> {
        let conn = self.pool.get()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM api_tokens WHERE token_hash=?1",
            params![token_hash],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn token_count(&self) -> Result<i64, EngineError> {
        let conn = self.pool.get()?;
        Ok(conn.query_row("SELECT COUNT(*) FROM api_tokens", [], |r| r.get(0))?)
    }

    pub fn get_config(&self, key: &str) -> Result<Option<String>, EngineError> {
        let conn = self.pool.get()?;
        let v = conn
            .query_row("SELECT value FROM config WHERE key=?1", params![key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        Ok(v)
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<(), EngineError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}
impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        name: r.get(1)?,
        status: r.get(2)?,
        total_tasks: r.get(3)?,
        done_tasks: r.get(4)?,
        failed_tasks: r.get(5)?,
        created_at: r.get(6)?,
        completed_at: r.get(7)?,
    })
}

fn task_from_row(row: TaskRow) -> Task {
    let status: TaskStatus =
        serde_json::from_str(&format!("\"{}\"", row.status)).unwrap_or(TaskStatus::Pending);
    Task {
        id: Id::from_str(&row.id).unwrap_or_default(),
        session_id: row.session_id.as_deref().and_then(|s| Id::from_str(s).ok()),
        title: row.title,
        content: row.content,
        voice: row.voice,
        model: row.model,
        style: row.style,
        status,
        priority: 0,
        total_chars: row.total_chars,
        total_tokens: row.total_tokens,
        total_chunks: row.total_chunks,
        done_chunks: row.done_chunks,
        failed_chunks: row.failed_chunks,
        output_path: None,
        duration_ms: row.duration_ms,
        provider_id: None,
        error: row.error,
        created_at: parse_dt(&row.created_at),
        updated_at: parse_dt(&row.updated_at),
        completed_at: row.completed_at.map(|s| parse_dt(&s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::create_test_pool;
    use crate::storage::migrate::run_migrations;

    fn setup() -> Storage {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        Storage::new(pool)
    }

    fn make_task(s: &Storage, title: &str) -> Task {
        let task = Task::new(mimotts_core::domain::CreateTaskInput {
            session_id: None,
            title: title.into(),
            content: "你好世界。".into(),
            voice: "冰糖".into(),
            model: "mimo-v2.5-tts".into(),
            style: None,
            priority: 0,
            provider_id: None,
        });
        s.insert_task(&task).unwrap();
        task
    }

    #[test]
    fn provider_seeded_and_key_roundtrip() {
        let s = setup();
        let providers = s.providers().unwrap();
        assert_eq!(providers.len(), 4);
        s.set_provider_key("xiaomi", "v1:sealed", true).unwrap();
        let p = s.provider("xiaomi").unwrap().unwrap();
        assert_eq!(p.api_key_sealed, "v1:sealed");
        assert!(p.is_configured);
    }

    #[test]
    fn task_pagination_and_filters() {
        let s = setup();
        for i in 0..25 {
            make_task(&s, &format!("task-{i:02}"));
        }
        let (rows, total) = s.tasks(0, 10, None, None, None).unwrap();
        assert_eq!(rows.len(), 10);
        assert_eq!(total, 25);
        let (rows2, total2) = s.tasks(0, 10, Some("pending"), None, None).unwrap();
        assert_eq!(total2, 25);
        assert_eq!(rows2.len(), 10);
        let (rows3, total3) = s.tasks(0, 50, None, None, Some("task-0")).unwrap();
        assert_eq!(total3, 10, "search 'task-0' matches task-00..09");
        assert_eq!(rows3.len(), 10);
    }

    #[test]
    fn chunk_claim_is_atomic() {
        let s = setup();
        let task = make_task(&s, "claim-test");
        let chunks = vec![
            Chunk::new(task.id.clone(), 1, "一。".into(), 10),
            Chunk::new(task.id.clone(), 2, "二。".into(), 10),
        ];
        s.insert_chunks(&chunks).unwrap();
        let cid = chunks[0].id.to_string();
        assert!(s.claim_chunk(&cid).unwrap());
        assert!(!s.claim_chunk(&cid).unwrap(), "second claim must fail");
        s.finish_chunk(&cid, "/tmp/a.wav", None, 500).unwrap();
        let rows = s.chunks(&task.id.to_string()).unwrap();
        assert_eq!(rows[0].status, "done");
        assert_eq!(rows[0].duration_ms, Some(500));
    }

    #[test]
    fn session_progress_terminates() {
        let s = setup();
        let sess = s.create_session("批").unwrap();
        s.add_session_total(&sess.id.to_string(), 2).unwrap();
        s.update_session_progress(&sess.id.to_string(), 1, 0).unwrap();
        s.update_session_progress(&sess.id.to_string(), 1, 0).unwrap();
        let row = s.session(&sess.id.to_string()).unwrap().unwrap();
        assert_eq!(row.status, "completed");
        assert_eq!(row.done_tasks, 2);
    }

    #[test]
    fn stale_inflight_reset() {
        let s = setup();
        let task = make_task(&s, "stale");
        let chunk = Chunk::new(task.id.clone(), 1, "一。".into(), 5);
        s.insert_chunks(std::slice::from_ref(&chunk)).unwrap();
        let cid = chunk.id.to_string();
        assert!(s.claim_chunk(&cid).unwrap());
        // fresh inflight survives
        assert_eq!(s.reset_stale_inflight(120).unwrap(), 0);
        // age it beyond threshold
        let old = (Utc::now() - chrono::Duration::seconds(300)).to_rfc3339();
        let conn = s.pool.get().unwrap();
        conn.execute(
            "UPDATE chunks SET updated_at=?1 WHERE id=?2",
            params![old, cid],
        )
        .unwrap();
        assert_eq!(s.reset_stale_inflight(120).unwrap(), 1);
    }

    #[test]
    fn reset_all_chunks_reclaims_done_files() {
        let s = setup();
        let task = make_task(&s, "reset-all");
        let chunks = vec![
            Chunk::new(task.id.clone(), 1, "一。".into(), 10),
            Chunk::new(task.id.clone(), 2, "二。".into(), 10),
        ];
        s.insert_chunks(&chunks).unwrap();
        let cid = chunks[0].id.to_string();
        assert!(s.claim_chunk(&cid).unwrap());
        s.finish_chunk(&cid, "/tmp/a.pcm", Some((0, 500)), 500).unwrap();
        // Re-chunk reset must wipe EVERY chunk (done included) and hand back
        // their DISTINCT audio files for on-disk reclamation.
        let files = s.reset_all_chunks(&task.id.to_string()).unwrap();
        assert_eq!(files, vec!["/tmp/a.pcm".to_string()]);
        assert!(s.chunks(&task.id.to_string()).unwrap().is_empty());
        assert_eq!(s.max_chunk_seq(&task.id.to_string()).unwrap(), 0);
    }
}
