use r2d2::{Pool, CustomizeConnection};
use r2d2_sqlite::SqliteConnectionManager;
use crate::shared::error::AppError;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Sets per-connection PRAGMAs: `busy_timeout=5000` (prevents SQLITE_BUSY under
/// concurrent writes — default 0ms causes immediate failure + 500ms retry cycles)
/// and `foreign_keys=ON` (not persisted in database file, must be set per conn).
#[derive(Debug)]
struct BusyTimeoutCustomizer;

impl CustomizeConnection<rusqlite::Connection, rusqlite::Error> for BusyTimeoutCustomizer {
    fn on_acquire(&self, conn: &mut rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-64000;
             PRAGMA temp_store=MEMORY;"
        )?;
        Ok(())
    }
}

pub fn create_pool(db_path: &str, max_size: u32) -> Result<DbPool, AppError> {
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder()
        .max_size(max_size)
        .connection_customizer(Box::new(BusyTimeoutCustomizer))
        .build(manager)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    Ok(pool)
}

pub fn create_test_pool() -> DbPool {
    // Use shared-cache in-memory DB with a unique name per pool so that:
    // 1. All connections WITHIN the same pool share the same database.
    // 2. Different pools (different tests) get isolated databases.
    let db_name = format!("file:memdb_{}?mode=memory&cache=shared", fastrand::u64(..));
    let manager = SqliteConnectionManager::file(&db_name);
    let pool = Pool::builder()
        .max_size(30)
        .connection_customizer(Box::new(BusyTimeoutCustomizer))
        .build(manager)
        .unwrap();
    let conn = pool.get().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    pool
}
