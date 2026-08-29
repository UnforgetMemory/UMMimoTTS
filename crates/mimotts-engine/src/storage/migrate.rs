//! Schema v4 migration + provider seeding.

use rusqlite::Connection;

use crate::error::EngineError;

pub const PROVIDER_SEEDS: &[(&str, &str, &str)] = &[
    ("xiaomi", "小米 MIMO", "https://api.xiaomimimo.com/v1"),
    ("xiaomi-token-plan-cn", "小米 Token 计划(中国)", "https://token-plan-cn.xiaomimimo.com/v1"),
    ("xiaomi-token-plan-ams", "小米 Token 计划(美西)", "https://token-plan-ams.xiaomimimo.com/v1"),
    ("xiaomi-token-plan-sgp", "小米 Token 计划(新加坡)", "https://token-plan-sgp.xiaomimimo.com/v1"),
];

pub fn run_migrations(conn: &Connection) -> Result<(), EngineError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS providers (
            id             TEXT PRIMARY KEY,
            name           TEXT NOT NULL,
            base_url       TEXT NOT NULL,
            kind           TEXT NOT NULL DEFAULT 'xiaomi',
            api_key_sealed TEXT NOT NULL DEFAULT '',
            budget_group   TEXT NOT NULL DEFAULT 'default',
            is_configured  INTEGER NOT NULL DEFAULT 0,
            is_default     INTEGER NOT NULL DEFAULT 0,
            created_at     TEXT NOT NULL,
            updated_at     TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            status        TEXT NOT NULL DEFAULT 'active',
            total_tasks   INTEGER NOT NULL DEFAULT 0,
            done_tasks    INTEGER NOT NULL DEFAULT 0,
            failed_tasks  INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            completed_at  TEXT
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id            TEXT PRIMARY KEY,
            session_id    TEXT,
            title         TEXT NOT NULL,
            content       TEXT NOT NULL,
            voice         TEXT NOT NULL,
            model         TEXT NOT NULL,
            style         TEXT,
            status        TEXT NOT NULL DEFAULT 'pending',
            priority      INTEGER NOT NULL DEFAULT 0,
            total_chars   INTEGER NOT NULL DEFAULT 0,
            total_tokens  INTEGER NOT NULL DEFAULT 0,
            total_chunks  INTEGER NOT NULL DEFAULT 0,
            done_chunks   INTEGER NOT NULL DEFAULT 0,
            failed_chunks INTEGER NOT NULL DEFAULT 0,
            output_path   TEXT,
            duration_ms   INTEGER,
            provider_id   TEXT,
            error         TEXT,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            completed_at  TEXT,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id             TEXT PRIMARY KEY,
            task_id        TEXT NOT NULL,
            seq            INTEGER NOT NULL,
            text           TEXT NOT NULL,
            token_estimate INTEGER NOT NULL DEFAULT 0,
            status         TEXT NOT NULL DEFAULT 'pending',
            retry_count    INTEGER NOT NULL DEFAULT 0,
            audio_path     TEXT,
            audio_offset   INTEGER,
            audio_len      INTEGER,
            duration_ms    INTEGER,
            error          TEXT,
            created_at     TEXT NOT NULL,
            updated_at     TEXT NOT NULL,
            completed_at   TEXT,
            UNIQUE(task_id, seq),
            FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS api_tokens (
            id         TEXT PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            label      TEXT NOT NULL DEFAULT 'default',
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS config (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tasks_status_created ON tasks(status, created_at);
        CREATE INDEX IF NOT EXISTS idx_tasks_session ON tasks(session_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_status_created ON chunks(status, created_at);
        CREATE INDEX IF NOT EXISTS idx_chunks_task_seq ON chunks(task_id, seq);
        ",
    )?;

    let now = chrono::Utc::now().to_rfc3339();
    for (i, (id, name, base)) in PROVIDER_SEEDS.iter().enumerate() {
        let is_default = i64::from(i == 0);
        conn.execute(
            "INSERT OR IGNORE INTO providers
             (id, name, base_url, kind, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'xiaomi', ?4, ?5, ?5)",
            rusqlite::params![id, name, base, is_default, now],
        )?;
    }
    // v4 schema evolution: audio ranges (chunk audio lives in one shared raw
    // stream per task; offset/len address it). Idempotent for existing DBs.
    for (col, typ) in [("audio_offset", "INTEGER"), ("audio_len", "INTEGER")] {
        if let Err(e) = conn.execute(
            &format!("ALTER TABLE chunks ADD COLUMN {col} {typ}"),
            [],
        ) {
            let msg = e.to_string();
            if !msg.contains("duplicate column name") {
                return Err(EngineError::Internal(format!("chunks.{col} migrate: {e}")));
            }
        }
    }
    Ok(())
}

/// Import legacy v3 tasks (best effort) — used by `mimotts migrate`.
/// v3 stored statuses as JSON-quoted strings; strip quotes and lowercase.
pub fn import_legacy_tasks(conn: &Connection) -> Result<usize, EngineError> {
    let rows = conn
        .prepare("SELECT id, title, content, voice, model, status FROM tasks")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .and_then(|m| m.collect::<Result<Vec<_>, _>>())
        });
    let mut imported = 0usize;
    if let Ok(list) = rows {
        for (id, title, content, voice, model, raw_status) in list {
            let status = normalize_status(&raw_status);
            if status == "cancelled" {
                continue;
            }
            let now = chrono::Utc::now().to_rfc3339();
            let n = conn.execute(
                "INSERT OR IGNORE INTO tasks
                 (id, title, content, voice, model, status, total_chars, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                rusqlite::params![
                    id,
                    title,
                    content,
                    voice,
                    model,
                    status,
                    content.chars().count() as i64,
                    now
                ],
            )?;
            // Count only real inserts (INSERT OR IGNORE no-ops on duplicates).
            if n > 0 {
                imported += 1;
            }
        }
    }
    Ok(imported)
}

/// "\\\"Processing\\\"" / "Processing" / "\"queued\"" → v4 lowercase bare.
fn normalize_status(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').to_lowercase();
    match trimmed.as_str() {
        "processing" | "chunking" | "synthesizing" => "synthesizing".into(),
        "mergingfailed" | "failed" => "failed".into(),
        "done" | "completed" => "done".into(),
        "queued" => "queued".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_status_normalization() {
        assert_eq!(normalize_status("\"Processing\""), "synthesizing");
        assert_eq!(normalize_status("\"mergingfailed\""), "failed");
        assert_eq!(normalize_status("queued"), "queued");
        assert_eq!(normalize_status("\"Done\""), "done");
    }

    #[test]
    fn seeds_have_unique_ids() {
        let ids: Vec<&str> = PROVIDER_SEEDS.iter().map(|p| p.0).collect();
        let mut uniq = ids.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), ids.len());
    }
}
