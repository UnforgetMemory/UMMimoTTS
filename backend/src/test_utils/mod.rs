use crate::db::SqlitePool;
use crate::models::batch::BatchGroup;
use crate::models::task::{TaskStatus, TtsTask};
use crate::state::app_state::AppState;
use actix_web::web;
use chrono::Utc;

/// Create an in-memory SQLite pool for tests.
pub fn test_db() -> SqlitePool {
    crate::db::init_db_pool_in_memory()
}

/// Create a task fixture and persist it to the given database.
///
/// `status` must be a valid TaskStatus variant name (e.g. "pending", "completed", "failed").
pub fn fixture_task(db: &SqlitePool, id: &str, title: &str, status: &str) -> TtsTask {
    let task = TtsTask {
        id: id.to_string(),
        custom_title: Some(title.to_string()),
        status: serde_json::from_str::<TaskStatus>(&format!("\"{}\"", status))
            .expect("valid task status string"),
        model: "test-model".to_string(),
        voice: Some("test-voice".to_string()),
        text: "Hello test".to_string(),
        context: None,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
        audio_data: None,
        error: None,
        progress: 0.0,
        token_count: 0,
        char_count: 10,
        audio_duration_secs: None,
        audio_path: None,
        total_chunks: Some(1),
        current_chunk: Some(0),
        group_id: None,
        api_key: None,
    };
    crate::db::insert_task(db, &task);
    task
}

/// Create a batch-group fixture and persist it to the given database.
///
/// `status` must be a valid GroupStatus variant name (e.g. "pending", "processing", "completed").
pub fn fixture_group(
    db: &SqlitePool,
    id: &str,
    name: &str,
    status: &str,
    task_ids: Vec<String>,
) -> BatchGroup {
    let mut group = BatchGroup::new(
        name.to_string(),
        Some("test-voice".to_string()),
        "test-model".to_string(),
        None,
    );
    group.id = id.to_string();
    group.status = serde_json::from_str(&format!("\"{}\"", status))
        .expect("valid group status string");
    group.task_ids = task_ids;
    crate::db::insert_group(db, &group);
    group
}

/// Build a test AppState wrapping the given pool.
pub fn create_test_app_state(db: &SqlitePool) -> web::Data<AppState> {
    web::Data::new(AppState::new_with_pool(db.clone(), String::new()))
}
