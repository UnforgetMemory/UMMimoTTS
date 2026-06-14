//! E2E integration tests for task endpoints.
//!
//! Tests cover task CRUD, enqueue, and continue operations.

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
use um_mimo_tts_server::infra::persistence::provider_repo::{ProviderRepo, SqliteProviderRepo};
use um_mimo_tts_server::infra::queue::task_queue::TaskQueue;
use um_mimo_tts_server::infra::queue::chunk_queue::ChunkQueue;
use um_mimo_tts_server::infra::queue::rate_limiter::{TokenBucket, ProviderRateLimiterMap};
use um_mimo_tts_server::infra::queue::provider_balancer::ProviderLoadBalancer;
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
        let provider_repo: Arc<dyn ProviderRepo> =
            Arc::new(SqliteProviderRepo::new(pool.clone()));
        let _ = provider_repo.update_api_key("xiaomi", "test-key");

        let sse_bus = Arc::new(SseBus::new());

        let base_url: String = $base_url.to_string();

        let chunker = MimoChunker::new(&base_url, 2000, 5000);

        let (event_tx, _event_rx) =
            tokio::sync::broadcast::channel::<DomainEvent>(256);

        let client = Arc::new(MimoClient::new(&base_url));
        let cache = Arc::new(Cache::new(
            std::path::PathBuf::from("/tmp/test-cache"),
            Duration::from_secs(3600),
            100,
        ));
        let rate_limiter = Arc::new(TokenBucket::new(100));
        let token_budget = Arc::new(TokenBucket::new(1_000_000));
        let provider_rate_limiters = Arc::new(ProviderRateLimiterMap::new(1000, 10_000_000, 10));
        let load_balancer = Arc::new(ProviderLoadBalancer::new());

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
            provider_repo.clone(),
            provider_rate_limiters,
            load_balancer,
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
            provider_repo,
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
// Macro: create a task and return its id
// ---------------------------------------------------------------------------
macro_rules! create_task {
    ($app:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v2/tasks")
            .set_json(&json!({
                "content": "Hello world",
                "title": "Test task",
                "voice": "冰糖"
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
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
async fn test_create_task() {
    let app = build_app!("http://localhost:1");

    let req = test::TestRequest::post()
        .uri("/api/v2/tasks")
        .set_json(&json!({
            "content": "Hello world",
            "title": "Test task",
            "voice": "冰糖"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = body_json(resp).await;
    assert_eq!(body["title"], "Test task");
    assert_eq!(body["voice"], um_mimo_tts_server::constants::DEFAULT_VOICE);
    assert_eq!(body["status"], "pending");
    assert_eq!(body["model"], um_mimo_tts_server::constants::DEFAULT_MODEL);
    assert!((body["speed"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!(body["id"].is_string());
}

#[actix_web::test]
async fn test_create_task_defaults() {
    let app = build_app!("http://localhost:1");

    // Only required fields
    let req = test::TestRequest::post()
        .uri("/api/v2/tasks")
        .set_json(&json!({
            "content": "Hello world",
            "title": "Defaults test",
            "voice": "male-1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = body_json(resp).await;
    assert_eq!(body["model"], um_mimo_tts_server::constants::DEFAULT_MODEL);
    assert!((body["speed"].as_f64().unwrap() - 1.0).abs() < 1e-9);
}

#[actix_web::test]
async fn test_get_task() {
    let app = build_app!("http://localhost:1");
    let task_id = create_task!(&app);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v2/tasks/{}", task_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["id"].as_str().unwrap(), task_id);
    assert_eq!(body["title"], "Test task");
    assert_eq!(body["status"], "pending");
}

#[actix_web::test]
async fn test_get_task_not_found() {
    let app = build_app!("http://localhost:1");

    let req = test::TestRequest::get()
        .uri("/api/v2/tasks/nonexistent-id")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_list_tasks() {
    let app = build_app!("http://localhost:1");

    // Create two tasks
    let _id1 = create_task!(&app);
    let _id2 = create_task!(&app);

    let req = test::TestRequest::get()
        .uri("/api/v2/tasks")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert!(body["data"].is_array());
    assert!(body["data"].as_array().unwrap().len() >= 2);
}

#[actix_web::test]
async fn test_enqueue_task_with_mock() {
    // Start wiremock for the tokenize endpoint
    let ms = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/tokenize"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "sentences": [
                {"text": "Hello world", "token_count": 5, "char_count": 11}
            ]
        })))
        .mount(&ms)
        .await;

    let app = build_app!(ms.uri());
    let task_id = create_task!(&app);

    // Enqueue the task
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/tasks/{}/enqueue", task_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["ok"], true);
}

#[actix_web::test]
async fn test_continue_task() {
    // Start wiremock for the tokenize endpoint (needed for enqueue)
    let ms = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/tokenize"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "sentences": [
                {"text": "Hello world", "token_count": 5, "char_count": 11}
            ]
        })))
        .mount(&ms)
        .await;

    let app = build_app!(ms.uri());
    let task_id = create_task!(&app);

    // Enqueue first to create chunks in the DB
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/tasks/{}/enqueue", task_id))
        .to_request();
    let _ = test::call_service(&app, req).await;

    // Now continue the task (re-enqueues existing chunks)
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/tasks/{}/continue", task_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = body_json(resp).await;
    assert_eq!(body["ok"], true);
}
