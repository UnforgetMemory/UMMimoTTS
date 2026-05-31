//! E2E integration tests for batch endpoints.
//!
//! Tests cover the full batch lifecycle:
//! create, get, update status, add/update/delete items, and submit.

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
// Accepts a base URL string (e.g. a wiremock URI) used for the MimoChunker /
// MimoClient so that enqueue/submit operations can mock the tokenize endpoint.
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

        let task_service = Arc::new(TaskService::new(task_repo, chunk_repo, task_queue));

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
// Macro: create a batch and return its id
// ---------------------------------------------------------------------------
macro_rules! create_batch {
    ($app:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v2/batches")
            .set_json(&json!({
                "title": "E2E Batch",
                "voice": "female-1"
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let val: Value = body_json(resp).await;
        val["id"].as_str().unwrap().to_string()
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
async fn test_create_batch() {
    let app = build_app!("http://localhost:1");

    let req = test::TestRequest::post()
        .uri("/api/v2/batches")
        .set_json(&json!({
            "title": "Test Batch",
            "voice": "female-1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["title"], "Test Batch");
    assert!(body["id"].is_string());
    assert_eq!(body["status"], "preparing");
    assert_eq!(body["voice"], "female-1");
    assert_eq!(body["model"], "tts-1");
    assert!((body["speed"].as_f64().unwrap() - 1.0).abs() < 1e-9);
}

#[actix_web::test]
async fn test_get_batch() {
    let app = build_app!("http://localhost:1");
    let batch_id = create_batch!(&app);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v2/batches/{}", batch_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["id"].as_str().unwrap(), batch_id);
    assert_eq!(body["title"], "E2E Batch");
}

#[actix_web::test]
async fn test_get_batch_not_found() {
    let app = build_app!("http://localhost:1");

    let req = test::TestRequest::get()
        .uri("/api/v2/batches/nonexistent-id")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_update_batch_status() {
    let app = build_app!("http://localhost:1");
    let batch_id = create_batch!(&app);

    // Transition from Preparing -> Queued
    let req = test::TestRequest::put()
        .uri(&format!("/api/v2/batches/{}", batch_id))
        .set_json(&json!({"status": "queued"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["updated"], true);
}

#[actix_web::test]
async fn test_update_batch_invalid_status() {
    let app = build_app!("http://localhost:1");
    let batch_id = create_batch!(&app);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v2/batches/{}", batch_id))
        .set_json(&json!({"status": "InvalidStatus"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    // Should return a client error (serde parse failure)
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_add_batch_item() {
    let app = build_app!("http://localhost:1");
    let batch_id = create_batch!(&app);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{}/items", batch_id))
        .set_json(&json!({
            "seq": 1,
            "filename": "test.txt",
            "content": "Hello world"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = body_json(resp).await;
    assert_eq!(body["ok"], true);
}

#[actix_web::test]
async fn test_update_batch_item() {
    let app = build_app!("http://localhost:1");
    let batch_id = create_batch!(&app);

    // Add item first
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{}/items", batch_id))
        .set_json(&json!({
            "seq": 1,
            "filename": "test.txt",
            "content": "Hello world"
        }))
        .to_request();
    let _ = test::call_service(&app, req).await;

    // Update the item
    let req = test::TestRequest::put()
        .uri(&format!("/api/v2/batches/{}/items/{}", batch_id, 1))
        .set_json(&json!({
            "title": "Updated title"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["ok"], true);
}

#[actix_web::test]
async fn test_delete_batch_item() {
    let app = build_app!("http://localhost:1");
    let batch_id = create_batch!(&app);

    // Add item first
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{}/items", batch_id))
        .set_json(&json!({
            "seq": 1,
            "filename": "test.txt",
            "content": "Hello world"
        }))
        .to_request();
    let _ = test::call_service(&app, req).await;

    // Delete the item
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v2/batches/{}/items/{}", batch_id, 1))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["ok"], true);
}

#[actix_web::test]
async fn test_submit_batch_full() {
    // Start wiremock for the tokenize endpoint
    let ms = wiremock::MockServer::start().await;

    // Respond to POST /v1/tokenize with sentences
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/tokenize"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "sentences": [
                {"text": "Item 1 content here", "token_count": 5, "char_count": 18},
                {"text": "Item 2 content here", "token_count": 5, "char_count": 18}
            ]
        })))
        .mount(&ms)
        .await;

    let app = build_app!(ms.uri());

    // Create batch
    let batch_id = create_batch!(&app);

    // Add 2 items
    for seq in 1..=2 {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v2/batches/{}/items", batch_id))
            .set_json(&json!({
                "seq": seq,
                "filename": format!("file{}.txt", seq),
                "content": format!("Item {} content here", seq)
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // Submit the batch
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{}/submit", batch_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    // Should be an array of TaskSummary objects
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 2);

    for entry in body.as_array().unwrap() {
        assert!(entry["id"].is_string());
        assert!(entry["title"].is_string());
    }
}
