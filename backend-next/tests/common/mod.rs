//! Shared test helpers — inlined in each test file.

#![allow(dead_code)]

use um_mimo_tts_server_v3::infra::persistence::db::DbPool;
use um_mimo_tts_server_v3::infra::persistence::migrate::run_migrations;

/// Create a test DB pool with migrations applied.
pub fn create_db() -> DbPool {
    let pool = db_internal::create_test_pool();
    let conn = pool.get().unwrap();
    run_migrations(&conn).unwrap();
    pool
}

/// Re-export the internal db module for access to create_test_pool.
mod db_internal {
    pub use um_mimo_tts_server_v3::infra::persistence::db::create_test_pool;
}
