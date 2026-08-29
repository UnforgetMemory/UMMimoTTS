//! SQLite schema v4 (ADR-010) — flat sessions → tasks → chunks.
//!
//! Statuses are bare lowercase strings (no JSON quoting — v3 bug fixed).
//! PRAGMAs tuned for low memory: WAL, busy_timeout, small per-conn cache,
//! small pool (single local daemon).

use r2d2::{CustomizeConnection, Pool};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::error::EngineError;

pub type DbPool = Pool<SqliteConnectionManager>;

#[derive(Debug)]
struct Pragmas;
impl CustomizeConnection<Connection, rusqlite::Error> for Pragmas {
    fn on_acquire(&self, conn: &mut Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;
             PRAGMA synchronous=NORMAL;
             PRAGMA journal_mode=WAL;
             PRAGMA cache_size=-2000;   -- 2MB per connection (v3 used 64MB × 24)
             PRAGMA temp_store=MEMORY;",
        )
    }
}

/// Pool size 4 (single-writer local daemon). ADR perf budget: page cache ≤ 32MB total.
pub fn create_pool(db_path: &str) -> Result<DbPool, EngineError> {
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder()
        .max_size(4)
        .connection_customizer(Box::new(Pragmas))
        .build(manager)
        .map_err(|e| EngineError::Internal(e.to_string()))?;
    Ok(pool)
}

/// In-memory pool for tests.
pub fn create_test_pool() -> DbPool {
    let name = format!("file:memdb_{}?mode=memory&cache=shared", fastrand::u64(..));
    let manager = SqliteConnectionManager::file(&name);
    let pool = Pool::builder()
        .max_size(4)
        .connection_customizer(Box::new(Pragmas))
        .build(manager)
        .unwrap();
    pool
}
