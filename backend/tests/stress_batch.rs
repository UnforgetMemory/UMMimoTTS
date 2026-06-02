//! Full-link stress test: batch creation → processing → completion.
//!
//! Tests 100 / 1000 items through the whole pipeline with a mocked MIMO
//! API (wiremock). Measures how long each phase takes and asserts all
//! items complete successfully.
//!
//! Run:
//!   cargo test --test stress_batch stress_100  -- --nocapture
//!   cargo test --test stress_batch stress_1000 -- --nocapture  # ~2 min

#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::{test, web, App, http::StatusCode};
use serde_json::{json, Value};

use um_mimo_tts_server::domain::events::DomainEvent;
use um_mimo_tts_server::infra::cache::Cache;
use um_mimo_tts_server::infra::mimo::chunker::MimoChunker;
use um_mimo_tts_server::infra::mimo::client::MimoClient;
use um_mimo_tts_server::infra::persistence::batch_repo::{BatchRepo, SqliteBatchRepo};
use um_mimo_tts_server::infra::persistence::chunk_repo::{ChunkRepo, SqliteChunkRepo};
use um_mimo_tts_server::infra::persistence::db::create_test_pool;
use um_mimo_tts_server::infra::persistence::group_repo::{GroupRepo, SqliteGroupRepo};
use um_mimo_tts_server::infra::persistence::migrate::run_migrations;
use um_mimo_tts_server::infra::persistence::task_repo::{SqliteTaskRepo, TaskRepo};
use um_mimo_tts_server::infra::queue::chunk_queue::ChunkQueue;
use um_mimo_tts_server::infra::queue::rate_limiter::TokenBucket;
use um_mimo_tts_server::infra::queue::task_queue::TaskQueue;
use um_mimo_tts_server::infra::sse_bus::SseBus;
use um_mimo_tts_server::routes::AppState;
use um_mimo_tts_server::service::batch_service::BatchService;
use um_mimo_tts_server::service::group_service::GroupService;
use um_mimo_tts_server::service::task_service::TaskService;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use base64::Engine;

// ═══════════════════════════════════════════════════════════════════════
// Mock WAV — same bytes as client.rs unit test
// ═══════════════════════════════════════════════════════════════════════

const MOCK_WAV_BYTES: &[u8] = &[
    0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x41,
    0x56, 0x45, 0x66, 0x6d, 0x74, 0x20, 0x10, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x44, 0xac, 0x00, 0x00, 0x88, 0x58,
    0x01, 0x00, 0x02, 0x00, 0x10, 0x00, 0x64, 0x61, 0x74, 0x61,
    0x00, 0x00, 0x00, 0x00,
];

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

/// Generate deterministic text of varying **character** count (100..10 000).
fn gen_text(i: usize) -> String {
    let n_chars = 100 + (i * 99) % 9901;
    let pat = format!("{i}. 第{i}条压力测试。The quick brown fox jumps. ");
    let mut s = String::new();
    while s.chars().count() < n_chars {
        s.push_str(&pat);
    }
    // Truncate at character boundary
    let byte_idx = s.char_indices().nth(n_chars).map(|(i, _)| i).unwrap_or(s.len());
    s.truncate(byte_idx);
    s
}

/// Parse JSON from an actix test response.
async fn body_json(resp: actix_web::dev::ServiceResponse) -> Value {
    let bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Set up a wiremock that responds to POST /v1/chat/completions with
/// a minimal WAV (base64-encoded).
async fn mock_mimo_api() -> MockServer {
    let ms = MockServer::start().await;
    let wav_b64 = base64::engine::general_purpose::STANDARD.encode(MOCK_WAV_BYTES);

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "audio": { "data": wav_b64 }
                }
            }]
        })))
        .mount(&ms)
        .await;

    ms
}

// ═══════════════════════════════════════════════════════════════════════
// Stress-test runner
// ═══════════════════════════════════════════════════════════════════════

struct Metrics {
    n: usize,
    create: Duration,
    add: Duration,
    submit: Duration,
    process: Duration,
    total: Duration,
    ok: usize,
}

/// Create batch → add items → submit → poll completion → verify.
/// Uses the HTTP API for everything; the background enqueue in submit()
/// runs with the real 100 ms delay (for 1000 items that's ≈ 100 s).
async fn stress(n: usize, timeout: Duration) -> Metrics {
    // ── Wiremock ──────────────────────────────────────────────────
    let ms = mock_mimo_api().await;

    // ── DB + repos ────────────────────────────────────────────────
    let pool = create_test_pool();
    {
        let c = pool.get().unwrap();
        run_migrations(&c).unwrap();
    }

    let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
    let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
    let batch_repo: Arc<dyn BatchRepo> = Arc::new(SqliteBatchRepo::new(pool.clone()));
    let group_repo: Arc<dyn GroupRepo> = Arc::new(SqliteGroupRepo::new(pool.clone()));

    // ── Infrastructure ────────────────────────────────────────────
    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&ms.uri(), 2000, 5000);

    let cap = (n * 16).max(4096).min(65536);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(cap);

    let client = Arc::new(MimoClient::new("test-key", &ms.uri()));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/test-cache"),
        Duration::from_secs(3600),
        100,
    ));

    // High rate so the pipeline is the bottleneck, not the API throttle.
    let rl = Arc::new(TokenBucket::new(100_000));
    let tb = Arc::new(TokenBucket::new(10_000_000));

    // ── Queues ────────────────────────────────────────────────────
    let cq = Arc::new(ChunkQueue::new(
        pool.clone(),
        chunk_repo.clone(),
        task_repo.clone(),
        client,
        cache,
        rl,
        tb,
        tx.clone(),
        20,                        // max_concurrent
        n.min(200),                // max_active_tasks
        Duration::from_secs(60),
        std::path::PathBuf::from("/tmp/test-cache"),
    ));

    let tq = Arc::new(TaskQueue::new(
        pool.clone(),
        task_repo.clone(),
        chunk_repo.clone(),
        cq.clone(),
        group_repo.clone(),
        tx.clone(),
        chunker,
    ));

    // ── Services ──────────────────────────────────────────────────
    let ts = Arc::new(TaskService::new(
        task_repo.clone(),
        chunk_repo.clone(),
        tq.clone(),
        tx.clone(),
    ));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(
        batch_repo.clone(),
        ts.clone(),
        sse_bus.clone(),
    ));

    let state = AppState {
        batch_service: bs,
        task_service: ts,
        group_service: gs,
        sse_bus,
    };

    // ── Start workers + event listener ────────────────────────────
    cq.run_workers();
    let tq2 = tq.clone();
    tokio::spawn(async move { tq2.listen(rx).await; });

    // ── Actix test app (with 50 MB JSON limit for batch add) ──────
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            // 50 MB JSON limit (matches src/main.rs)
            .app_data(web::JsonConfig::default().limit(50 * 1024 * 1024))
            .configure(um_mimo_tts_server::routes::configure),
    )
    .await;

    let t0 = Instant::now();

    // ══════════════════════════════════════════════════════════════
    // 1. CREATE BATCH
    // ══════════════════════════════════════════════════════════════
    let req = test::TestRequest::post()
        .uri("/api/v2/batches")
        .set_json(&json!({"title": format!("stress-{n}"), "voice": um_mimo_tts_server::constants::DEFAULT_VOICE}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let batch: Value = body_json(resp).await;
    let bid = batch["id"].as_str().unwrap().to_string();
    let t1 = t0.elapsed();

    // ══════════════════════════════════════════════════════════════
    // 2. ADD ITEMS (chunked batch-add)
    // ══════════════════════════════════════════════════════════════
    const ADD_CHUNK: usize = 500;

    {
        let mut added = 0usize;
        while added < n {
            let hi = (added + ADD_CHUNK).min(n);
            let items: Vec<Value> = (added..hi)
                .map(|i| json!({
                    "seq": (i + 1) as i32,
                    "filename": format!("f{i}.txt"),
                    "content": gen_text(i),
                }))
                .collect();

            let req = test::TestRequest::post()
                .uri(&format!("/api/v2/batches/{bid}/items/batch"))
                .set_json(&items)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::CREATED, "add chunk {added}..{hi}");
            added = hi;
        }
    }
    let t2 = t0.elapsed();

    // ══════════════════════════════════════════════════════════════
    // 3. SUBMIT
    // ══════════════════════════════════════════════════════════════
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/submit"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tasks: Value = body_json(resp).await;
    let task_arr = tasks.as_array().unwrap();
    assert_eq!(task_arr.len(), n, "submit returned wrong task count");
    let mut t3 = t0.elapsed();

    // ══════════════════════════════════════════════════════════════
    // 4. POLL UNTIL COMPLETED
    // ══════════════════════════════════════════════════════════════
    let deadline = Instant::now() + timeout;
    let mut prev = String::new();
    let mut polls = 0u64;

    loop {
        assert!(
            Instant::now() < deadline,
            "TIMEOUT after {timeout:?} bid={bid} status={prev}"
        );

        let req = test::TestRequest::get()
            .uri(&format!("/api/v2/batches/{bid}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let b: Value = body_json(resp).await;
        let st = b["status"].as_str().unwrap_or_default().to_string();
        polls += 1;

        if st != prev {
            eprintln!("[stress] batch={st}  elapsed={:?}  polls={polls}", t0.elapsed());
            prev = st.clone();
        }

        match st.as_str() {
            "completed" => {
                let t4 = t0.elapsed();

                // ── Verify (paginate through all tasks) ────────────
                let mut ok_cnt = 0usize;
                let mut fail_cnt = 0usize;
                let mut other: Vec<String> = vec![];
                let mut page = 0i64;
                let page_size = 5000i64;
                loop {
                    let req = test::TestRequest::get()
                        .uri(&format!("/api/v2/tasks?batch_id={bid}&page={page}&page_size={page_size}"))
                        .to_request();
                    let resp = test::call_service(&app, req).await;
                    let tl: Value = body_json(resp).await;
                    let data = tl["data"].as_array().unwrap();
                    if data.is_empty() { break; }

                    for t in data {
                        let s = t["status"].as_str().unwrap_or_default();
                        match s {
                            "done" => ok_cnt += 1,
                            "failed" => fail_cnt += 1,
                            other_status => other.push(other_status.to_string()),
                        }
                    }
                    page += 1;
                }

                eprintln!(
                    "[stress] DONE  ok={ok_cnt}/{n}  fail={fail_cnt}  other={other:?}  total={t4:?}"
                );
                assert_eq!(ok_cnt + fail_cnt + other.len(), n, "task count mismatch");
                assert_eq!(fail_cnt, 0, "expected 0 failed");
                assert!(other.is_empty(), "non-terminal tasks: {other:?}");

                return Metrics {
                    n,
                    create: t1,
                    add: t2 - t1,
                    submit: t3 - t2,
                    process: t4 - t3,
                    total: t4,
                    ok: ok_cnt,
                };
            }
            "failed" | "cancelled" => {
                panic!("batch {bid} ended '{st}' at {:?}", t0.elapsed());
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn stress_100() {
    let m = stress(100, Duration::from_secs(60)).await;
    println!(
        "STRESS_RESULT items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(60), "took too long: {:.1?}", m.total);
}

#[actix_web::test]
#[ignore]
async fn stress_1000() {
    let m = stress(1000, Duration::from_secs(300)).await;
    println!(
        "STRESS_RESULT items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(300), "took too long: {:.1?}", m.total);
}

/// 10000 items — heavy stress.
/// WARNING: ~17 minutes due to 100ms enqueue delay in submit().
#[actix_web::test]
#[ignore]
async fn stress_10000() {
    let m = stress(10000, Duration::from_secs(1800)).await;
    println!(
        "STRESS_RESULT items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(1800), "took too long: {:.1?}", m.total);
}

// ═══════════════════════════════════════════════════════════════════════
// Large-text stress tests
// ═══════════════════════════════════════════════════════════════════════

/// Single task with very long text (100K characters) — tests chunking pipeline
/// under heavy per-task load. This exercises token estimation, chunk splitting,
/// and sequential chunk processing within one task.
#[actix_web::test]
async fn stress_large_text_100k() {
    let ms = mock_mimo_api().await;
    let pool = create_test_pool();
    { let c = pool.get().unwrap(); run_migrations(&c).unwrap(); }

    let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
    let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
    let batch_repo: Arc<dyn BatchRepo> = Arc::new(SqliteBatchRepo::new(pool.clone()));
    let group_repo: Arc<dyn GroupRepo> = Arc::new(SqliteGroupRepo::new(pool.clone()));
    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&ms.uri(), 2000, 5000);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(4096);
    let client = Arc::new(MimoClient::new("test-key", &ms.uri()));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/test-cache-lg"), Duration::from_secs(3600), 100,
    ));
    let rl = Arc::new(TokenBucket::new(100_000));
    let tb = Arc::new(TokenBucket::new(10_000_000));

    let cq = Arc::new(ChunkQueue::new(
        pool.clone(), chunk_repo.clone(), task_repo.clone(), client, cache,
        rl, tb, tx.clone(), 20, 20, Duration::from_secs(60),
        std::path::PathBuf::from("/tmp/test-cache-lg"),
    ));
    let tq = Arc::new(TaskQueue::new(
        pool.clone(), task_repo.clone(), chunk_repo.clone(), cq.clone(),
        group_repo.clone(), tx.clone(), chunker,
    ));
    let ts = Arc::new(TaskService::new(task_repo.clone(), chunk_repo.clone(), tq.clone(), tx.clone()));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(batch_repo.clone(), ts.clone(), sse_bus.clone()));

    let state = AppState { batch_service: bs, task_service: ts, group_service: gs, sse_bus };
    cq.run_workers();
    let tq2 = tq.clone();
    tokio::spawn(async move { tq2.listen(rx).await; });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .app_data(web::JsonConfig::default().limit(50 * 1024 * 1024))
            .configure(um_mimo_tts_server::routes::configure),
    ).await;

    // 1. Create batch
    let t0 = Instant::now();
    let req = test::TestRequest::post().uri("/api/v2/batches").set_json(&json!({
        "title": "large-text-100k",
        "voice": "冰糖",
        "model": "mimo-v2.5-tts",
    })).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let b: Value = body_json(resp).await;
    let bid = b["id"].as_str().unwrap().to_string();

    // 2. Add one item with 100K chars
    let large_text = "这是压力测试文本。".repeat(12500); // 12500 * 8 chars = 100K chars
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/items/batch"))
        .set_json(&json!([{
            "seq": 1,
            "filename": "large_100k.txt",
            "content": large_text,
        }]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 3. Submit
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/submit"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tasks: Value = body_json(resp).await;
    let task_arr = tasks.as_array().unwrap();
    assert_eq!(task_arr.len(), 1, "should have exactly 1 task");
    let task_id = task_arr[0]["id"].as_str().unwrap().to_string();
    eprintln!("[large-text] batch={bid} task={task_id} text_len={}", large_text.len());

    // 4. Poll until completed
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut prev = String::new();
    loop {
        assert!(Instant::now() < deadline, "TIMEOUT after 120s bid={bid} status={prev}");

        let req = test::TestRequest::get()
            .uri(&format!("/api/v2/batches/{bid}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let b: Value = body_json(resp).await;
        let st = b["status"].as_str().unwrap_or_default().to_string();

        if st != prev {
            eprintln!("[large-text] batch={st}  elapsed={:?}  text_len={}", t0.elapsed(), large_text.len());
            prev = st.clone();
        }

        match st.as_str() {
            "completed" => {
                // Verify task is done
                let req = test::TestRequest::get()
                    .uri(&format!("/api/v2/tasks/{task_id}"))
                    .to_request();
                let resp = test::call_service(&app, req).await;
                let t: Value = body_json(resp).await;
                let task_status = t["status"].as_str().unwrap_or_default();
                assert_eq!(task_status, "done", "task should be done, got '{task_status}'");

                let total = t["total_chunks"].as_i64().unwrap_or(0);
                let done = t["done_chunks"].as_i64().unwrap_or(0);
                eprintln!(
                    "[large-text] DONE  chunks={done}/{total} total={:?} text_len={}",
                    t0.elapsed(), large_text.len()
                );
                assert!(total > 1, "should produce multiple chunks, got {total}");
                assert_eq!(done, total, "all chunks should be done");
                return;
            }
            "failed" | "cancelled" => {
                panic!("batch {bid} ended '{st}'");
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }
}

/// Single task with 500K characters — extreme test of chunking + rate limiting.
#[actix_web::test]
async fn stress_large_text_500k() {
    let ms = mock_mimo_api().await;
    let pool = create_test_pool();
    { let c = pool.get().unwrap(); run_migrations(&c).unwrap(); }

    let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
    let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
    let batch_repo: Arc<dyn BatchRepo> = Arc::new(SqliteBatchRepo::new(pool.clone()));
    let group_repo: Arc<dyn GroupRepo> = Arc::new(SqliteGroupRepo::new(pool.clone()));
    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&ms.uri(), 2000, 5000);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(4096);
    let client = Arc::new(MimoClient::new("test-key", &ms.uri()));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/test-cache-xl"), Duration::from_secs(3600), 100,
    ));
    let rl = Arc::new(TokenBucket::new(100_000));
    let tb = Arc::new(TokenBucket::new(10_000_000));

    let cq = Arc::new(ChunkQueue::new(
        pool.clone(), chunk_repo.clone(), task_repo.clone(), client, cache,
        rl, tb, tx.clone(), 20, 20, Duration::from_secs(60),
        std::path::PathBuf::from("/tmp/test-cache-xl"),
    ));
    let tq = Arc::new(TaskQueue::new(
        pool.clone(), task_repo.clone(), chunk_repo.clone(), cq.clone(),
        group_repo.clone(), tx.clone(), chunker,
    ));
    let ts = Arc::new(TaskService::new(task_repo.clone(), chunk_repo.clone(), tq.clone(), tx.clone()));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(batch_repo.clone(), ts.clone(), sse_bus.clone()));

    let state = AppState { batch_service: bs, task_service: ts, group_service: gs, sse_bus };
    cq.run_workers();
    let tq2 = tq.clone();
    tokio::spawn(async move { tq2.listen(rx).await; });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .app_data(web::JsonConfig::default().limit(50 * 1024 * 1024))
            .configure(um_mimo_tts_server::routes::configure),
    ).await;

    let t0 = Instant::now();
    let req = test::TestRequest::post().uri("/api/v2/batches").set_json(&json!({
        "title": "large-text-500k",
        "voice": "冰糖",
        "model": "mimo-v2.5-tts",
    })).to_request();
    let resp = test::call_service(&app, req).await;
    let b: Value = body_json(resp).await;
    let bid = b["id"].as_str().unwrap().to_string();

    let large_text = "这是压力测试文本，用于测试超大文本分块处理能力。".repeat(20000); // ~200K chars
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/items/batch"))
        .set_json(&json!([{
            "seq": 1,
            "filename": "large_500k.txt",
            "content": large_text,
        }]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/submit"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tasks: Value = body_json(resp).await;
    let task_arr = tasks.as_array().unwrap();
    assert_eq!(task_arr.len(), 1);
    let task_id = task_arr[0]["id"].as_str().unwrap().to_string();
    eprintln!("[large-text-500k] batch={bid} task={task_id} text_len={}", large_text.len());

    let deadline = Instant::now() + Duration::from_secs(300);
    let mut prev = String::new();
    loop {
        assert!(Instant::now() < deadline, "TIMEOUT after 300s bid={bid}");

        let req = test::TestRequest::get()
            .uri(&format!("/api/v2/batches/{bid}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let b: Value = body_json(resp).await;
        let st = b["status"].as_str().unwrap_or_default().to_string();

        if st != prev {
            eprintln!("[large-text-500k] batch={st}  elapsed={:?}", t0.elapsed());
            prev = st.clone();
        }

        match st.as_str() {
            "completed" => {
                let req = test::TestRequest::get()
                    .uri(&format!("/api/v2/tasks/{task_id}"))
                    .to_request();
                let resp = test::call_service(&app, req).await;
                let t: Value = body_json(resp).await;
                let task_status = t["status"].as_str().unwrap_or_default();
                assert_eq!(task_status, "done");

                let total = t["total_chunks"].as_i64().unwrap_or(0);
                let done = t["done_chunks"].as_i64().unwrap_or(0);
                eprintln!(
                    "[large-text-500k] DONE  chunks={done}/{total} total={:?} text_len={}",
                    t0.elapsed(), large_text.len()
                );
                assert!(total > 1, "should produce multiple chunks, got {total}");
                assert_eq!(done, total, "all chunks should be done");
                return;
            }
            "failed" | "cancelled" => panic!("batch {bid} ended '{st}'"),
            _ => { tokio::time::sleep(Duration::from_millis(300)).await; }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Real API stress tests (no wiremock — contacts production MIMO API)
// ═══════════════════════════════════════════════════════════════════════
//
// These tests require env vars:
//   MIMO_API_KEY  — API key for the MIMO TTS service
//   MIMO_BASE_URL — Base URL (default: https://api.mimo.com)
//   MIMO_RATE_LIMIT_RPM — Rate limit (default: 120)
//
// ⚠ WARNING: These make real API calls that may incur charges.
// All real tests are #[ignore] by default — run explicitly:
//
//   cargo test --test stress_batch r100  -- --nocapture --ignored
//   cargo test --test stress_batch r1000 -- --nocapture --ignored  # ~expensive

/// Run a stress test against the real MIMO API.
/// Builds app inline (async) just like `stress()` but with env-configured MimoClient.
async fn stress_real(n: usize, timeout: Duration) -> Metrics {
    // Read env — will panic with a clear message if missing.
    let api_key = std::env::var("MIMO_API_KEY")
        .expect("REAL API test requires MIMO_API_KEY env var");
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| "https://api.mimo.com".to_string());
    let rpm: u64 = std::env::var("MIMO_RATE_LIMIT_RPM")
        .ok()
        .and_then(|v| v.parse().ok())
        // Default 20 RPM: MIMO enforces a **per-voice** rate limit of 20 RPM.
        // Setting 120 (per-app limit) causes 429s because all tasks share the
        // same voice (mimo_default). 20 RPM = ~600s for 200 chunks.
        .unwrap_or(20);

    eprintln!(
        "[stress_real] using MIMO_BASE_URL={base_url} RPM={rpm} api_key=***{}",
        &api_key[api_key.len().saturating_sub(4)..]
    );

    // ── DB + repos ────────────────────────────────────────────────
    let pool = create_test_pool();
    {
        let c = pool.get().unwrap();
        run_migrations(&c).unwrap();
    }

    let batch_repo: Arc<dyn BatchRepo> = Arc::new(SqliteBatchRepo::new(pool.clone()));
    let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
    let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
    let group_repo: Arc<dyn GroupRepo> = Arc::new(SqliteGroupRepo::new(pool.clone()));

    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&base_url, 2000, 5000);

    let cap = (n * 16).max(4096).min(65536);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(cap);

    let client = Arc::new(MimoClient::new(&api_key, &base_url));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/ummimo-cache-real"),
        Duration::from_secs(3600),
        100,
    ));

    // Rate limit to the configured RPM so we don't exceed the API quota.
    let rl = Arc::new(TokenBucket::new(rpm));
    // Token budget: effectively unlimited — the RPM rate limiter is what
    // throttles actual API calls. A low token budget would deadlock workers
    // because each chunk needs (text.len() / 2) tokens (≈ 100-2500), and
    // all workers would wait on notify() that never arrives.
    let tb = Arc::new(TokenBucket::new(10_000_000));

    // ── Queues ────────────────────────────────────────────────────────
    let cq = Arc::new(ChunkQueue::new(
        pool.clone(),
        chunk_repo.clone(),
        task_repo.clone(),
        client,
        cache,
        rl,
        tb,
        tx.clone(),
        20,
        n.min(200),
        Duration::from_secs(60),
        std::path::PathBuf::from("/tmp/ummimo-cache-real"),
    ));

    let tq = Arc::new(TaskQueue::new(
        pool.clone(),
        task_repo.clone(),
        chunk_repo.clone(),
        cq.clone(),
        group_repo.clone(),
        tx.clone(),
        chunker,
    ));

    // ── Services ──────────────────────────────────────────────────────
    let ts = Arc::new(TaskService::new(
        task_repo.clone(),
        chunk_repo.clone(),
        tq.clone(),
        tx.clone(),
    ));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(
        batch_repo.clone(),
        ts.clone(),
        sse_bus.clone(),
    ));

    let state = AppState {
        batch_service: bs,
        task_service: ts,
        group_service: gs,
        sse_bus,
    };

    // ── Start workers + event listener ────────────────────────────────
    cq.run_workers();
    let tq2 = tq.clone();
    tokio::spawn(async move { tq2.listen(rx).await; });

    // ── Actix test app ───────────────────────────────────────────────
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .app_data(web::JsonConfig::default().limit(50 * 1024 * 1024))
            .configure(um_mimo_tts_server::routes::configure),
    )
    .await;

    let t0 = Instant::now();

    // ══════════════════════════════════════════════════════════════════
    // 1. CREATE BATCH
    // ══════════════════════════════════════════════════════════════════
    let req = test::TestRequest::post()
        .uri("/api/v2/batches")
        .set_json(&json!({"title": format!("stress-real-{n}"), "voice": "mimo_default", "model": "mimo-v2.5-tts"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let batch: Value = body_json(resp).await;
    let bid = batch["id"].as_str().unwrap().to_string();
    let t1 = t0.elapsed();

    // ══════════════════════════════════════════════════════════════════
    // 2. ADD ITEMS (chunked batch-add)
    // ══════════════════════════════════════════════════════════════════
    const ADD_CHUNK: usize = 500;
    {
        let mut added = 0usize;
        while added < n {
            let hi = (added + ADD_CHUNK).min(n);
            let items: Vec<Value> = (added..hi)
                .map(|i| json!({
                    "seq": (i + 1) as i32,
                    "filename": format!("f{i}.txt"),
                    "content": gen_text(i),
                }))
                .collect();

            let req = test::TestRequest::post()
                .uri(&format!("/api/v2/batches/{bid}/items/batch"))
                .set_json(&items)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::CREATED, "add chunk {added}..{hi}");
            added = hi;
        }
    }
    let t2 = t0.elapsed();

    // ══════════════════════════════════════════════════════════════════
    // 3. SUBMIT
    // ══════════════════════════════════════════════════════════════════
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/submit"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tasks: Value = body_json(resp).await;
    let task_arr = tasks.as_array().unwrap();
    assert_eq!(task_arr.len(), n, "submit returned wrong task count");
    let mut t3 = t0.elapsed();
    eprintln!("[stress_real] submit returned {} tasks in {:?}", task_arr.len(), t3 - t2);

    // ══════════════════════════════════════════════════════════════════
    // 4. POLL UNTIL COMPLETED
    // ══════════════════════════════════════════════════════════════════
    let deadline = Instant::now() + timeout;
    let mut prev = String::new();
    let mut polls = 0u64;
    let mut last_task_poll = Instant::now();

    loop {
        assert!(
            Instant::now() < deadline,
            "TIMEOUT after {timeout:?} bid={bid} status={prev}"
        );

        let req = test::TestRequest::get()
            .uri(&format!("/api/v2/batches/{bid}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let b: Value = body_json(resp).await;
        let st = b["status"].as_str().unwrap_or_default().to_string();
        polls += 1;

        if st != prev {
            eprintln!("[stress_real] batch={st}  elapsed={:?}  polls={polls}", t0.elapsed());
            prev = st.clone();
        }

        // Every 10s, also poll task status to see if enqueuing is making progress
        if last_task_poll.elapsed() > Duration::from_secs(10) {
            last_task_poll = Instant::now();
            let req = test::TestRequest::get()
                .uri(&format!("/api/v2/tasks?batch_id={bid}&page=0&page_size=10"))
                .to_request();
            let resp = test::call_service(&app, req).await;
            let tl: Value = body_json(resp).await;
            let data = tl["data"].as_array().unwrap();
            if !data.is_empty() {
                let statuses: Vec<String> = data.iter()
                    .map(|t| t["status"].as_str().unwrap_or("?").to_string())
                    .collect();
                eprintln!("[stress_real] sample tasks: {:?}", statuses);
            }
        }

        match st.as_str() {
            "completed" => {
                let t4 = t0.elapsed();

                // ── Verify (paginate tasks) ──────────────────────────
                let mut ok_cnt = 0usize;
                let mut fail_cnt = 0usize;
                let mut other: Vec<String> = vec![];
                let mut page = 0i64;
                let page_size = 5000i64;
                loop {
                    let req = test::TestRequest::get()
                        .uri(&format!("/api/v2/tasks?batch_id={bid}&page={page}&page_size={page_size}"))
                        .to_request();
                    let resp = test::call_service(&app, req).await;
                    let tl: Value = body_json(resp).await;
                    let data = tl["data"].as_array().unwrap();
                    if data.is_empty() { break; }

                    for t in data {
                        let s = t["status"].as_str().unwrap_or_default();
                        match s {
                            "done" => ok_cnt += 1,
                            "failed" => fail_cnt += 1,
                            other_status => other.push(other_status.to_string()),
                        }
                    }
                    page += 1;
                }

                eprintln!(
                    "[stress_real] DONE  ok={ok_cnt}/{n}  fail={fail_cnt}  other={other:?}  total={t4:?}"
                );
                assert_eq!(ok_cnt + fail_cnt + other.len(), n, "task count mismatch");
                assert_eq!(fail_cnt, 0, "expected 0 failed");
                assert!(other.is_empty(), "non-terminal tasks: {other:?}");

                return Metrics { n, create: t1, add: t2 - t1, submit: t3 - t2, process: t4 - t3, total: t4, ok: ok_cnt };
            }
            "failed" | "cancelled" => {
                panic!("batch {bid} ended '{st}' at {:?}", t0.elapsed());
            }
            _ => {
                // ── Fallback: if all tasks terminal, treat batch as done ──
                let mut ok_cnt = 0usize;
                let mut fail_cnt = 0usize;
                let mut all_terminal = true;
                let mut page = 0i64;
                const PSZ: i64 = 5000;
                loop {
                    let req = test::TestRequest::get()
                        .uri(&format!("/api/v2/tasks?batch_id={bid}&page={page}&page_size={PSZ}"))
                        .to_request();
                    let resp = test::call_service(&app, req).await;
                    let tl: Value = body_json(resp).await;
                    let data = tl["data"].as_array().unwrap();
                    if data.is_empty() { break; }
                    for t in data {
                        let s = t["status"].as_str().unwrap_or_default();
                        match s {
                            "done" => ok_cnt += 1,
                            "failed" => fail_cnt += 1,
                            _ => all_terminal = false,
                        }
                    }
                    page += 1;
                }
                if all_terminal && (ok_cnt + fail_cnt) == n {
                    eprintln!("[stress_real] all-terminal fallback ok={ok_cnt}/{n} fail={fail_cnt}");
                    let t4 = t0.elapsed();
                    assert_eq!(fail_cnt, 0, "expected 0 failed");
                    return Metrics { n, create: t1, add: t2 - t1, submit: t3 - t2, process: t4 - t3, total: t4, ok: ok_cnt };
                }
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        }
    }
}

/***********************************************************************
 * Real API stress tests (no wiremock — contacts real MIMO API)
 *
 * ⚠  WARNING: These make real API calls that may incur charges.
 * All are #[ignore] by default — run explicitly:
 *
 *   cargo test --test stress_batch r100   -- --nocapture --ignored
 *   cargo test --test stress_batch r1000  -- --nocapture --ignored
 *
 * Env vars:
 *   MIMO_API_KEY       (required)
 *   MIMO_BASE_URL      (default: https://api.mimo.com)
 *   MIMO_RATE_LIMIT_RPM (default: 120)
 **********************************************************************/

#[actix_web::test]
#[ignore]
/// 100 items via real MIMO API (small-scale validation)
async fn r100() {
    let m = stress_real(100, Duration::from_secs(600)).await;
    println!(
        "STRESS_REAL items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(300), "took too long: {:.1?}", m.total);
}

#[actix_web::test]
#[ignore]
/// 1000 items via real MIMO API (expect ~20-30 min due to 120 RPM)
async fn r1000() {
    let m = stress_real(1000, Duration::from_secs(3600)).await;
    println!(
        "STRESS_REAL items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(3600), "took too long: {:.1?}", m.total);
}

#[actix_web::test]
#[ignore]
/// 10000 items via real MIMO API (expect ~2+ hours — VERY expensive!)
/// Run at your own risk — you will be charged for ~10000 TTS API calls.
async fn r10000() {
    let m = stress_real(10000, Duration::from_secs(7200)).await;
    println!(
        "STRESS_REAL items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(7200), "took too long: {:.1?}", m.total);
}
