//! E2E integration tests for group endpoints.
//!
//! Tests cover creating groups under a batch and listing groups by batch.

#![allow(dead_code)]

use actix_web::{test, web, App, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

use um_mimo_tts_server::infra::persistence::db::create_test_pool;
use um_mimo_tts_server::infra::persistence::migrate::run_migrations;
use um_mimo_tts_server::infra::persistence::task_repo::SqliteTaskRepo;
use um_mimo_tts_server::infra::persistence::chunk_repo::SqliteChunkRepo;
use um_mimo_tts_server::infra::persistence::batch_repo::SqliteBatchRepo;
use um_mimo_tts_server::infra::persistence::group_repo::SqliteGroupRepo;
use um_mimo_tts_server::infra::persistence::batch_repo::BatchRepo;
use um_mimo_tts_server::infra::persistence::chunk_repo::ChunkRepo;
use um_mimo_tts_server::infra::persistence::task_repo::TaskRepo;
use um_mimo_tts_server::infra::persistence::group_repo::GroupRepo;
use um_mimo_tts_server::infra::queue::task_queue::TaskQueue;
use um_mimo_tts_server::infra::queue::chunk_queue::ChunkQueue;
use um_mimo_tts_server::infra::queue::rate_limiter::TokenBucket;
use um_mimo_tts_server::infra::mimo::chunker::MimoChunker;
use um_mimo_tts_server::infra::mimo::client::MimoClient;
use um_mimo_tts_server::infra::cache::Cache;
use um_mimo_tts_server::infra::sse_bus::SseBus;
use um_mimo_tts_server::domain::events::DomainEvent;
use um_mimo_tts_server::service::task_service::TaskService;
use um_mimo_tts_server::service::batch_service::BatchService;
use um_mimo_tts_server::service::group_service::GroupService;
use um_mimo_tts_server::routes::AppState;

// ---------------------------------------------------------------------------
// Macro: build a fully-wired actix-web test app
// ---------------------------------------------------------------------------
macro_rules! build_app {
    ($base_url:expr) => {{
        let pool = create_test_pool();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();

        let task_repo: Arc<dyn TaskRepo> =
            Arc::new(SqliteTaskRepo::new(pool.clone()));
        let chunk_repo: Arc<dyn ChunkRepo> =
            Arc::new(SqliteChunkRepo::new(pool.clone()));
        let batch_repo: Arc<dyn BatchRepo> =
            Arc::new(SqliteBatchRepo::new(pool.clone()));
        let group_repo: Arc<dyn GroupRepo> =
            Arc::new(SqliteGroupRepo::new(pool.clone()));

        let sse_bus = Arc::new(SseBus::new());

        let base_url: String = $base_url.to_string();

        let chunker = MimoChunker::new(&base_url, 2000, 5000);

        let (event_tx, _event_rx) =
            tokio::sync::broadcast::channel::<DomainEvent>(256);

        let client = Arc::new(MimoClient::new("test-key", &base_url));
        let cache = Arc::new(Cache::new(
            std::path::PathBuf::from("/tmp/test-cache"),
            Duration::from_secs(3600),
            100,
        ));
        let rate_limiter = Arc::new(TokenBucket::new(100));
        let token_budget = Arc::new(TokenBucket::new(1_000_000));

        let chunk_queue = Arc::new(ChunkQueue::new(
            pool.clone(),
            chunk_repo.clone(),
            task_repo.clone(),
            client,
            cache,
            rate_limiter,
            token_budget,
            event_tx.clone(),
            1,
            20,
            Duration::from_secs(30),
            std::path::PathBuf::from("/tmp/test-cache"),
        ));

        let task_queue = Arc::new(TaskQueue::new(
            pool.clone(),
            task_repo.clone(),
            chunk_repo.clone(),
            chunk_queue,
            group_repo.clone(),
            event_tx.clone(),
            chunker,
        ));

        let task_service = Arc::new(TaskService::new(task_repo, chunk_repo, task_queue, event_tx.clone()));

        let group_service = Arc::new(GroupService::new(group_repo));

        let batch_service = Arc::new(BatchService::new(
            batch_repo,
            task_service.clone(),
            sse_bus.clone(),
        ));

        let app_state = AppState {
            batch_service,
            task_service,
            group_service,
            sse_bus,
        };

        test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .configure(um_mimo_tts_server::routes::configure),
        )
        .await
    }};
}

// ---------------------------------------------------------------------------
// Helper: extract JSON body from a service response
// ---------------------------------------------------------------------------
async fn body_json(resp: actix_web::dev::ServiceResponse) -> Value {
    let bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ===========================================================================
// Tests
// ===========================================================================

#[actix_web::test]
async fn test_create_group() {
    let app = build_app!("http://localhost:1");

    // Create a batch first
    let req = test::TestRequest::post()
        .uri("/api/v2/batches")
        .set_json(&json!({
            "title": "Batch for Groups",
            "voice": "冰糖"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let batch: Value = body_json(resp).await;
    let batch_id = batch["id"].as_str().unwrap().to_string();

    // Create 2 groups under the batch
    for i in 1..=2 {
        let req = test::TestRequest::post()
            .uri("/api/v2/groups")
            .set_json(&json!({
                "batch_id": batch_id,
                "title": format!("Group {}", i)
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // List groups by batch_id
    let req = test::TestRequest::get()
        .uri(&format!("/api/v2/groups?batch_id={}", batch_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 1);
}
