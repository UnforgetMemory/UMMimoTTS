use crate::models::batch::{BatchGroup, GroupStatus};
use crate::models::task::{TaskStatus, TtsTask};
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::Path;

pub type SqlitePool = Pool<SqliteConnectionManager>;

/// Initialize the database connection pool
pub fn init_db_pool(db_path: &str) -> SqlitePool {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .expect("Failed to create SQLite pool");

    run_migrations(&pool);
    pool
}

/// Initialize an in-memory database pool (for tests)
pub fn init_db_pool_in_memory() -> SqlitePool {
    let manager = SqliteConnectionManager::memory();
    let pool = Pool::builder()
        .max_size(4)
        .build(manager)
        .expect("Failed to create in-memory SQLite pool");

    run_migrations(&pool);
    pool
}

/// Run CREATE TABLE IF NOT EXISTS migrations
fn run_migrations(pool: &SqlitePool) {
    let conn = pool.get().expect("Failed to get connection for migrations");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            custom_title TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            model TEXT NOT NULL DEFAULT '',
            voice TEXT,
            text TEXT NOT NULL DEFAULT '',
            context TEXT,
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            error TEXT,
            progress REAL NOT NULL DEFAULT 0.0,
            token_count INTEGER NOT NULL DEFAULT 0,
            char_count INTEGER NOT NULL DEFAULT 0,
            audio_duration_secs REAL,
            total_chunks INTEGER,
            current_chunk INTEGER,
            group_id TEXT,
            api_key TEXT
        );

        CREATE TABLE IF NOT EXISTS task_groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            voice TEXT,
            model TEXT NOT NULL DEFAULT '',
            context TEXT,
            created_at TEXT NOT NULL,
            total_tasks INTEGER NOT NULL DEFAULT 0,
            completed_tasks INTEGER NOT NULL DEFAULT 0,
            failed_tasks INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS group_tasks (
            group_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (group_id, task_id)
        );

        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
        CREATE INDEX IF NOT EXISTS idx_tasks_group_id ON tasks(group_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at);
        CREATE INDEX IF NOT EXISTS idx_group_tasks_group_id ON group_tasks(group_id);
        ",
    )
    .expect("Failed to run database migrations");
}

// ---------------------------------------------------------------------------
// Helper: convert a string column to DateTime<Utc>
// ---------------------------------------------------------------------------
fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|v| v.parse::<DateTime<Utc>>().ok())
}

fn fmt_dt(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn fmt_dt_opt(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(|d| d.to_rfc3339())
}

// ---------------------------------------------------------------------------
// Task CRUD
// ---------------------------------------------------------------------------

pub fn insert_task(pool: &SqlitePool, task: &TtsTask) {
    let conn = pool.get().expect("Failed to get DB connection");
    conn.execute(
        "INSERT INTO tasks (id, custom_title, status, model, voice, text, context, created_at, started_at, completed_at, error, progress, token_count, char_count, audio_duration_secs, total_chunks, current_chunk, group_id, api_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            task.id,
            task.custom_title,
            task.status.as_str(),
            task.model,
            task.voice,
            task.text,
            task.context,
            fmt_dt(&task.created_at),
            fmt_dt_opt(task.started_at),
            fmt_dt_opt(task.completed_at),
            task.error,
            task.progress,
            task.token_count as i64,
            task.char_count as i64,
            task.audio_duration_secs,
            task.total_chunks.map(|v| v as i64),
            task.current_chunk.map(|v| v as i64),
            task.group_id,
            task.api_key,
        ],
    )
    .unwrap_or_else(|e| { tracing::error!("Failed to insert task {}: {}", task.id, e); 0 });
}

pub fn update_task(pool: &SqlitePool, task: &TtsTask) {
    let conn = pool.get().expect("Failed to get DB connection");
    conn.execute(
        "UPDATE tasks SET custom_title=?1, status=?2, model=?3, voice=?4, text=?5, context=?6, created_at=?7, started_at=?8, completed_at=?9, error=?10, progress=?11, token_count=?12, char_count=?13, audio_duration_secs=?14, total_chunks=?15, current_chunk=?16, group_id=?17, api_key=?18 WHERE id=?19",
        params![
            task.custom_title,
            task.status.as_str(),
            task.model,
            task.voice,
            task.text,
            task.context,
            fmt_dt(&task.created_at),
            fmt_dt_opt(task.started_at),
            fmt_dt_opt(task.completed_at),
            task.error,
            task.progress,
            task.token_count as i64,
            task.char_count as i64,
            task.audio_duration_secs,
            task.total_chunks.map(|v| v as i64),
            task.current_chunk.map(|v| v as i64),
            task.group_id,
            task.api_key,
            task.id,
        ],
    )
    .unwrap_or_else(|e| { tracing::error!("Failed to update task {}: {}", task.id, e); 0 });
}

pub fn delete_task(pool: &SqlitePool, task_id: &str) {
    let conn = pool.get().expect("Failed to get DB connection");
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])
        .unwrap_or_else(|e| { tracing::error!("Failed to delete task {}: {}", task_id, e); 0 });
}

pub fn get_task_from_db(pool: &SqlitePool, task_id: &str) -> Option<TtsTask> {
    let conn = pool.get().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, custom_title, status, model, voice, text, context, created_at, started_at, completed_at, error, progress, token_count, char_count, audio_duration_secs, total_chunks, current_chunk, group_id, api_key FROM tasks WHERE id = ?1",
        )
        .ok()?;

    stmt.query_row(params![task_id], |row| {
        let status_str: String = row.get(2)?;
        let status = TaskStatus::from_str(&status_str).unwrap_or(TaskStatus::Pending);

        Ok(TtsTask {
            id: row.get(0)?,
            custom_title: row.get(1)?,
            status,
            model: row.get(3)?,
            voice: row.get(4)?,
            text: row.get(5)?,
            context: row.get(6)?,
            created_at: row.get::<_, String>(7)?.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
            started_at: parse_dt(row.get(8)?),
            completed_at: parse_dt(row.get(9)?),
            audio_data: None,
            error: row.get(10)?,
            progress: row.get(11)?,
            token_count: row.get::<_, i64>(12)? as usize,
            char_count: row.get::<_, i64>(13)? as usize,
            audio_duration_secs: row.get(14)?,
            audio_path: None,
            total_chunks: row.get::<_, Option<i64>>(15)?.map(|v| v as usize),
            current_chunk: row.get::<_, Option<i64>>(16)?.map(|v| v as usize),
            group_id: row.get(17)?,
            api_key: row.get(18)?,
        })
    })
    .ok()
}

pub fn list_tasks_from_db(
    pool: &SqlitePool,
    page: usize,
    per_page: usize,
    status_filter: Option<&str>,
    search: Option<&str>,
    sort: Option<&str>,
    group_id: Option<&str>,
) -> (Vec<TtsTask>, usize) {
    let conn = pool.get().expect("Failed to get DB connection");

    // Build query
    let mut where_clauses: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(s) = status_filter {
        if !s.is_empty() {
            where_clauses.push(format!("status = ?{}", param_idx));
            param_values.push(Box::new(s.to_string()));
            param_idx += 1;
        }
    }

    if let Some(s) = search {
        if !s.is_empty() {
            where_clauses.push(format!("custom_title LIKE ?{}", param_idx));
            param_values.push(Box::new(format!("%{}%", s)));
            param_idx += 1;
        }
    }

    if let Some(g) = group_id {
        if !g.is_empty() {
            where_clauses.push(format!("group_id = ?{}", param_idx));
            param_values.push(Box::new(g.to_string()));
            param_idx += 1;
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Count total
    let count_sql = format!("SELECT COUNT(*) FROM tasks {}", where_sql);
    let total: usize = conn
        .query_row(&count_sql, rusqlite::params_from_iter(param_values.iter().map(|p| p.as_ref())), |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0) as usize;

    // Sort order
    let order_clause = match sort.and_then(|s| {
        let s = s.trim();
        if s.is_empty() { None } else { Some(s) }
    }) {
        Some("created_at_asc") => "created_at ASC".to_string(),
        Some("created_at") | None => "created_at DESC".to_string(),
        Some("status") => "status ASC, created_at DESC".to_string(),
        Some("progress") => "progress ASC, created_at DESC".to_string(),
        Some(other) => format!("{} ASC, created_at DESC", other),
    };

    let offset = page * per_page;
    let query_sql = format!(
        "SELECT id, custom_title, status, model, voice, text, context, created_at, started_at, completed_at, error, progress, token_count, char_count, audio_duration_secs, total_chunks, current_chunk, group_id, api_key FROM tasks {} ORDER BY {} LIMIT ?{} OFFSET ?{}",
        where_sql, order_clause, param_idx, param_idx + 1
    );

    let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for p in param_values {
        all_params.push(p);
    }
    all_params.push(Box::new(per_page as i64));
    all_params.push(Box::new(offset as i64));

    let mut stmt = conn.prepare(&query_sql).expect("Failed to prepare list tasks query");
    let tasks = stmt
        .query_map(rusqlite::params_from_iter(all_params.iter().map(|p| p.as_ref())), |row| {
            let status_str: String = row.get(2)?;
            let status = TaskStatus::from_str(&status_str).unwrap_or(TaskStatus::Pending);

            Ok(TtsTask {
                id: row.get(0)?,
                custom_title: row.get(1)?,
                status,
                model: row.get(3)?,
                voice: row.get(4)?,
                text: row.get(5)?,
                context: row.get(6)?,
                created_at: row.get::<_, String>(7)?.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
                started_at: parse_dt(row.get(8)?),
                completed_at: parse_dt(row.get(9)?),
                audio_data: None,
                error: row.get(10)?,
                progress: row.get(11)?,
                token_count: row.get::<_, i64>(12)? as usize,
                char_count: row.get::<_, i64>(13)? as usize,
                audio_duration_secs: row.get(14)?,
                audio_path: None,
                total_chunks: row.get::<_, Option<i64>>(15)?.map(|v| v as usize),
                current_chunk: row.get::<_, Option<i64>>(16)?.map(|v| v as usize),
                group_id: row.get(17)?,
                api_key: row.get(18)?,
            })
        })
        .expect("Failed to query tasks");

    let tasks: Vec<TtsTask> = tasks.filter_map(|r| r.ok()).collect();

    (tasks, total)
}

pub fn load_all_tasks(pool: &SqlitePool) -> Vec<TtsTask> {
    let conn = pool.get().expect("Failed to get DB connection");
    let mut stmt = conn
        .prepare(
            "SELECT id, custom_title, status, model, voice, text, context, created_at, started_at, completed_at, error, progress, token_count, char_count, audio_duration_secs, total_chunks, current_chunk, group_id, api_key FROM tasks ORDER BY created_at DESC",
        )
        .expect("Failed to prepare load all tasks");

    let tasks = stmt
        .query_map([], |row| {
            let status_str: String = row.get(2)?;
            let status = TaskStatus::from_str(&status_str).unwrap_or(TaskStatus::Pending);

            Ok(TtsTask {
                id: row.get(0)?,
                custom_title: row.get(1)?,
                status,
                model: row.get(3)?,
                voice: row.get(4)?,
                text: row.get(5)?,
                context: row.get(6)?,
                created_at: row.get::<_, String>(7)?.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
                started_at: parse_dt(row.get(8)?),
                completed_at: parse_dt(row.get(9)?),
                audio_data: None,
                error: row.get(10)?,
                progress: row.get(11)?,
                token_count: row.get::<_, i64>(12)? as usize,
                char_count: row.get::<_, i64>(13)? as usize,
                audio_duration_secs: row.get(14)?,
                audio_path: None,
                total_chunks: row.get::<_, Option<i64>>(15)?.map(|v| v as usize),
                current_chunk: row.get::<_, Option<i64>>(16)?.map(|v| v as usize),
                group_id: row.get(17)?,
                api_key: row.get(18)?,
            })
        })
        .expect("Failed to load all tasks");

    tasks.filter_map(|r| r.ok()).collect()
}

// ---------------------------------------------------------------------------
// Group CRUD
// ---------------------------------------------------------------------------

// We use "task_groups" as the table name since "groups" is a SQL reserved word.

pub fn insert_group(pool: &SqlitePool, group: &BatchGroup) {
    let conn = pool.get().expect("Failed to get DB connection");
    conn.execute(
        "INSERT INTO task_groups (id, name, status, voice, model, context, created_at, total_tasks, completed_tasks, failed_tasks, total_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            group.id,
            group.name,
            group.status.as_str(),
            group.voice,
            group.model,
            group.context,
            fmt_dt(&group.created_at),
            group.total_tasks as i64,
            group.completed_tasks as i64,
            group.failed_tasks as i64,
            group.total_tokens as i64,
        ],
    )
    .unwrap_or_else(|e| { tracing::error!("Failed to insert group {}: {}", group.id, e); 0 });

    // Insert group-task associations
    for (i, task_id) in group.task_ids.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO group_tasks (group_id, task_id, sort_order) VALUES (?1, ?2, ?3)",
            params![group.id, task_id, i as i64],
        )
        .ok();
    }
}

pub fn update_group(pool: &SqlitePool, group: &BatchGroup) {
    let conn = pool.get().expect("Failed to get DB connection");
    conn.execute(
        "UPDATE task_groups SET name=?1, status=?2, voice=?3, model=?4, context=?5, created_at=?6, total_tasks=?7, completed_tasks=?8, failed_tasks=?9, total_tokens=?10 WHERE id=?11",
        params![
            group.name,
            group.status.as_str(),
            group.voice,
            group.model,
            group.context,
            fmt_dt(&group.created_at),
            group.total_tasks as i64,
            group.completed_tasks as i64,
            group.failed_tasks as i64,
            group.total_tokens as i64,
            group.id,
        ],
    )
    .unwrap_or_else(|e| { tracing::error!("Failed to update group {}: {}", group.id, e); 0 });

    // Update group-task associations: delete old, insert new
    conn.execute("DELETE FROM group_tasks WHERE group_id = ?1", params![group.id])
        .ok();
    for (i, task_id) in group.task_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO group_tasks (group_id, task_id, sort_order) VALUES (?1, ?2, ?3)",
            params![group.id, task_id, i as i64],
        )
        .ok();
    }
}

pub fn delete_group(pool: &SqlitePool, group_id: &str) {
    let conn = pool.get().expect("Failed to get DB connection");
    conn.execute("DELETE FROM task_groups WHERE id = ?1", params![group_id])
        .unwrap_or_else(|e| { tracing::error!("Failed to delete group {}: {}", group_id, e); 0 });
    conn.execute("DELETE FROM group_tasks WHERE group_id = ?1", params![group_id])
        .ok();
}

pub fn get_group_from_db(pool: &SqlitePool, group_id: &str) -> Option<BatchGroup> {
    let conn = pool.get().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, status, voice, model, context, created_at, total_tasks, completed_tasks, failed_tasks, total_tokens FROM task_groups WHERE id = ?1",
        )
        .ok()?;

    let mut group = stmt
        .query_row(params![group_id], |row| {
            let status_str: String = row.get(2)?;
            let status = GroupStatus::from_str(&status_str).unwrap_or(GroupStatus::Pending);

            Ok(BatchGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                status,
                voice: row.get(3)?,
                model: row.get(4)?,
                context: row.get(5)?,
                created_at: row.get::<_, String>(6)?.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
                task_ids: Vec::new(), // loaded separately
                total_tasks: row.get::<_, i64>(7)? as usize,
                completed_tasks: row.get::<_, i64>(8)? as usize,
                failed_tasks: row.get::<_, i64>(9)? as usize,
                total_tokens: row.get::<_, i64>(10)? as usize,
            })
        })
        .ok()?;

    // Load task_ids for this group
    if let Ok(mut stmt2) = conn.prepare("SELECT task_id FROM group_tasks WHERE group_id = ?1 ORDER BY sort_order ASC") {
        if let Ok(rows) = stmt2.query_map(params![group_id], |row| row.get::<_, String>(0)) {
            group.task_ids = rows.filter_map(|r| r.ok()).collect();
        }
    }

    Some(group)
}

pub fn list_groups_from_db(
    pool: &SqlitePool,
    page: usize,
    per_page: usize,
) -> (Vec<BatchGroup>, usize) {
    let conn = pool.get().expect("Failed to get DB connection");

    let total: usize = conn
        .query_row("SELECT COUNT(*) FROM task_groups", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let offset = page * per_page;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, status, voice, model, context, created_at, total_tasks, completed_tasks, failed_tasks, total_tokens FROM task_groups ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
        )
        .expect("Failed to prepare list groups query");

    let groups = stmt
        .query_map(params![per_page as i64, offset as i64], |row| {
            let status_str: String = row.get(2)?;
            let status = GroupStatus::from_str(&status_str).unwrap_or(GroupStatus::Pending);

            Ok(BatchGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                status,
                voice: row.get(3)?,
                model: row.get(4)?,
                context: row.get(5)?,
                created_at: row.get::<_, String>(6)?.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
                task_ids: Vec::new(),
                total_tasks: row.get::<_, i64>(7)? as usize,
                completed_tasks: row.get::<_, i64>(8)? as usize,
                failed_tasks: row.get::<_, i64>(9)? as usize,
                total_tokens: row.get::<_, i64>(10)? as usize,
            })
        })
        .expect("Failed to query groups");

    let groups: Vec<BatchGroup> = groups.filter_map(|r| r.ok()).collect();
    (groups, total)
}

pub fn load_all_groups(pool: &SqlitePool) -> Vec<BatchGroup> {
    let conn = pool.get().expect("Failed to get DB connection");
    let mut stmt = conn
        .prepare(
            "SELECT id, name, status, voice, model, context, created_at, total_tasks, completed_tasks, failed_tasks, total_tokens FROM task_groups ORDER BY created_at DESC",
        )
        .expect("Failed to prepare load all groups");

    let groups = stmt
        .query_map([], |row| {
            let status_str: String = row.get(2)?;
            let status = GroupStatus::from_str(&status_str).unwrap_or(GroupStatus::Pending);

            Ok(BatchGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                status,
                voice: row.get(3)?,
                model: row.get(4)?,
                context: row.get(5)?,
                created_at: row.get::<_, String>(6)?.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
                task_ids: Vec::new(),
                total_tasks: row.get::<_, i64>(7)? as usize,
                completed_tasks: row.get::<_, i64>(8)? as usize,
                failed_tasks: row.get::<_, i64>(9)? as usize,
                total_tokens: row.get::<_, i64>(10)? as usize,
            })
        })
        .expect("Failed to load all groups");

    let mut groups: Vec<BatchGroup> = groups.filter_map(|r| r.ok()).collect();

    // Load task_ids for each group
    for group in &mut groups {
        if let Ok(mut stmt2) = conn.prepare("SELECT task_id FROM group_tasks WHERE group_id = ?1 ORDER BY sort_order ASC") {
            if let Ok(rows) = stmt2.query_map(params![group.id], |row| row.get::<_, String>(0)) {
                group.task_ids = rows.filter_map(|r| r.ok()).collect();
            }
        }
    }

    groups
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

pub fn compute_stats(pool: &SqlitePool) -> super::models::response::StatsSummary {
    let conn = pool.get().expect("Failed to get DB connection");

    let total_tasks: usize = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let completed: usize = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE status = 'completed'", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let failed: usize = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE status = 'failed'", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let processing: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('pending', 'queued', 'synthesizing', 'streaming')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;

    let total_tokens: usize = conn
        .query_row("SELECT COALESCE(SUM(token_count), 0) FROM tasks", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let total_chars: usize = conn
        .query_row("SELECT COALESCE(SUM(char_count), 0) FROM tasks", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    super::models::response::StatsSummary {
        total_tasks,
        completed,
        failed,
        processing,
        total_tokens,
        total_chars,
    }
}

pub fn compute_group_stats(pool: &SqlitePool, group_id: &str) -> super::models::response::StatsSummary {
    let conn = pool.get().expect("Failed to get DB connection");

    let total_tasks: usize = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE group_id = ?1", params![group_id], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let completed: usize = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE group_id = ?1 AND status = 'completed'", params![group_id], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let failed: usize = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE group_id = ?1 AND status = 'failed'", params![group_id], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let processing: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE group_id = ?1 AND status IN ('pending', 'queued', 'synthesizing', 'streaming')",
            params![group_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;

    let total_tokens: usize = conn
        .query_row("SELECT COALESCE(SUM(token_count), 0) FROM tasks WHERE group_id = ?1", params![group_id], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    let total_chars: usize = conn
        .query_row("SELECT COALESCE(SUM(char_count), 0) FROM tasks WHERE group_id = ?1", params![group_id], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    super::models::response::StatsSummary {
        total_tasks,
        completed,
        failed,
        processing,
        total_tokens,
        total_chars,
    }
}
