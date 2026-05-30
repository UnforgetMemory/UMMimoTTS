use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use crate::shared::error::AppError;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn create_pool(db_path: &str, max_size: u32) -> Result<DbPool, AppError> {
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder().max_size(max_size).build(manager)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(pool)
}

pub fn create_test_pool() -> DbPool {
    let manager = SqliteConnectionManager::memory();
    let pool = Pool::builder().max_size(2).build(manager).unwrap();
    let conn = pool.get().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    pool
}
