use rusqlite::Connection;
use crate::shared::error::AppError;

pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS batches (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'preparing',
            total_tasks     INTEGER NOT NULL DEFAULT 0,
            done_tasks      INTEGER NOT NULL DEFAULT 0,
            failed_tasks    INTEGER NOT NULL DEFAULT 0,
            total_chars     INTEGER NOT NULL DEFAULT 0,
            total_tokens    INTEGER NOT NULL DEFAULT 0,
            default_voice   TEXT NOT NULL,
            default_model   TEXT NOT NULL,
            default_style   TEXT,
            default_speed   REAL NOT NULL DEFAULT 1.0,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            completed_at    TEXT
        );

        CREATE TABLE IF NOT EXISTS pending_items (
            id              TEXT PRIMARY KEY,
            batch_id        TEXT NOT NULL,
            seq             INTEGER NOT NULL,
            filename        TEXT NOT NULL,
            content         TEXT NOT NULL,
            text_preview    TEXT NOT NULL,
            total_chars     INTEGER NOT NULL DEFAULT 0,
            token_estimate  INTEGER NOT NULL DEFAULT 0,
            custom_title    TEXT,
            custom_voice    TEXT,
            custom_model    TEXT,
            custom_style    TEXT,
            custom_speed    REAL,
            effective_title    TEXT NOT NULL,
            effective_voice    TEXT NOT NULL,
            effective_model    TEXT NOT NULL,
            effective_style    TEXT,
            effective_speed    REAL NOT NULL DEFAULT 1.0,
            status          TEXT NOT NULL DEFAULT 'pending',
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            UNIQUE(batch_id, seq),
            FOREIGN KEY (batch_id) REFERENCES batches(id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id              TEXT PRIMARY KEY,
            task_type       TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'pending',
            group_id        TEXT,
            batch_id        TEXT,
            content         TEXT NOT NULL,
            content_ref     TEXT,
            title           TEXT NOT NULL,
            voice           TEXT NOT NULL,
            model           TEXT NOT NULL,
            style           TEXT,
            speed           REAL NOT NULL DEFAULT 1.0,
            priority        INTEGER NOT NULL DEFAULT 0,
            total_chars     INTEGER NOT NULL DEFAULT 0,
            total_tokens    INTEGER NOT NULL DEFAULT 0,
            total_chunks    INTEGER NOT NULL DEFAULT 0,
            done_chunks     INTEGER NOT NULL DEFAULT 0,
            failed_chunks   INTEGER NOT NULL DEFAULT 0,
            output_path     TEXT,
            output_duration REAL,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            completed_at    TEXT
        );

        CREATE TABLE IF NOT EXISTS batch_tasks (
            id              TEXT PRIMARY KEY,
            batch_id        TEXT NOT NULL,
            child_task_id   TEXT NOT NULL UNIQUE,
            seq             INTEGER NOT NULL,
            FOREIGN KEY (child_task_id) REFERENCES tasks(id)
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id              TEXT PRIMARY KEY,
            task_id         TEXT NOT NULL,
            seq             INTEGER NOT NULL,
            status          TEXT NOT NULL DEFAULT 'pending',
            text            TEXT NOT NULL,
            char_count      INTEGER NOT NULL DEFAULT 0,
            token_count     INTEGER NOT NULL DEFAULT 0,
            retry_count     INTEGER NOT NULL DEFAULT 0,
            max_retries     INTEGER NOT NULL DEFAULT 3,
            priority        INTEGER NOT NULL DEFAULT 0,
            audio_path      TEXT,
            duration_ms     INTEGER,
            error_message   TEXT,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            UNIQUE(task_id, seq),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        );

        CREATE TABLE IF NOT EXISTS groups (
            id              TEXT PRIMARY KEY,
            batch_id        TEXT,
            name            TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'pending',
            voice           TEXT,
            model           TEXT,
            style           TEXT,
            speed           REAL,
            priority        INTEGER NOT NULL DEFAULT 0,
            total_tasks     INTEGER NOT NULL DEFAULT 0,
            done_tasks      INTEGER NOT NULL DEFAULT 0,
            failed_tasks    INTEGER NOT NULL DEFAULT 0,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            completed_at    TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_tasks_batch ON tasks(batch_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_group ON tasks(group_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
        CREATE INDEX IF NOT EXISTS idx_chunks_task ON chunks(task_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_status ON chunks(status);
        CREATE INDEX IF NOT EXISTS idx_chunks_priority ON chunks(priority);
        CREATE INDEX IF NOT EXISTS idx_batch_tasks_batch ON batch_tasks(batch_id);
        CREATE INDEX IF NOT EXISTS idx_pending_items_batch ON pending_items(batch_id);
        CREATE INDEX IF NOT EXISTS idx_groups_batch ON groups(batch_id);
    ")?;

    // Add columns to batches table for existing databases
    let _ = conn.execute("ALTER TABLE batches ADD COLUMN total_chars INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE batches ADD COLUMN total_tokens INTEGER NOT NULL DEFAULT 0", []);

    Ok(())
}
