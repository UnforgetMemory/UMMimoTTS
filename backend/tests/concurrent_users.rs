//! Concurrent virtual-user tests: N users simultaneously create batches,
//! add items, submit, and poll until completion — all through the HTTP API
//! against a wiremock-backed MIMO mock.
//!
//! Uses `tokio::join!` for cooperative concurrency on the same task,
//! avoiding `spawn_local`'s `'static` requirement.
//!
//! Run:
//!   cargo test --test concurrent_users -- --nocapture

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
use um_mimo_tts_server::infra::persistence::provider_repo::{ProviderRepo, SqliteProviderRepo};
use um_mimo_tts_server::infra::queue::chunk_queue::ChunkQueue;
use um_mimo_tts_server::infra::queue::rate_limiter::{TokenBucket, ProviderRateLimiterMap};
use um_mimo_tts_server::infra::queue::provider_balancer::ProviderLoadBalancer;
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
// Mock WAV
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

fn gen_text(i: usize) -> String {
    let n_chars = 100 + (i * 99) % 901;
    let pat = format!("{i}. 用户{i}测试文本。Quick brown fox. ");
    let mut s = String::new();
    while s.chars().count() < n_chars { s.push_str(&pat); }
    let byte_idx = s.char_indices().nth(n_chars).map(|(i, _)| i).unwrap_or(s.len());
    s.truncate(byte_idx);
    s
}

async fn body_json(resp: actix_web::dev::ServiceResponse) -> Value {
    let bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn mock_mimo_api() -> MockServer {
    let ms = MockServer::start().await;
    let wav_b64 = base64::engine::general_purpose::STANDARD.encode(MOCK_WAV_BYTES);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "audio": { "data": wav_b64 } } }]
        })))
        .mount(&ms).await;
    ms
}

// ═══════════════════════════════════════════════════════════════════════
// Metrics
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone)]
struct UserMetrics {
    user_id: usize,
    batch_id: String,
    create_ms: u128,
    add_items_ms: u128,
    submit_ms: u128,
    process_ms: u128,
    total_ms: u128,
    ok_count: usize,
    fail_count: usize,
    error: Option<String>,
}

struct AggregateReport {
    user_count: usize,
    total_items: usize,
    wall_clock_ms: u128,
    throughput_per_sec: f64,
    avg_total_ms: f64,
    p50_ms: u128,
    p95_ms: u128,
    error_rate: f64,
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    if sorted.is_empty() { return 0; }
    let idx = ((sorted.len() as f64 * p).ceil() as usize).min(sorted.len()) - 1;
    sorted[idx]
}

fn aggregate(metrics: &[UserMetrics], wall: Duration) -> AggregateReport {
    let n = metrics.len();
    let total_items: usize = metrics.iter().map(|m| m.ok_count + m.fail_count).sum();
    let total_ok: usize = metrics.iter().map(|m| m.ok_count).sum();
    let errors: usize = metrics.iter().filter(|m| m.error.is_some()).count();
    let mut totals: Vec<u128> = metrics.iter().map(|m| m.total_ms).collect();
    totals.sort();
    let avg = if n > 0 { totals.iter().sum::<u128>() as f64 / n as f64 } else { 0.0 };
    let wall_s = wall.as_secs_f64();
    let tp = if wall_s > 0.0 { total_ok as f64 / wall_s } else { 0.0 };
    AggregateReport {
        user_count: n, total_items,
        wall_clock_ms: wall.as_millis(),
        throughput_per_sec: tp, avg_total_ms: avg,
        p50_ms: percentile(&totals, 0.50),
        p95_ms: percentile(&totals, 0.95),
        error_rate: if n > 0 { errors as f64 / n as f64 } else { 0.0 },
    }
}

fn print_report(r: &AggregateReport) {
    let sep = "=".repeat(70);
    eprintln!("\n{sep}");
    eprintln!("  CONCURRENT USER TEST REPORT  (Users={}, Items={})", r.user_count, r.total_items);
    eprintln!("{sep}");
    eprintln!("  Wall clock:   {:.1}s", r.wall_clock_ms as f64 / 1000.0);
    eprintln!("  Throughput:   {:.1} items/sec", r.throughput_per_sec);
    eprintln!("  avg={:.0}ms  p50={}ms  p95={}ms", r.avg_total_ms, r.p50_ms, r.p95_ms);
    eprintln!("  error_rate:   {:.1}%", r.error_rate * 100.0);
    eprintln!("{sep}\n");
}

// ═══════════════════════════════════════════════════════════════════════
// Build shared app macro
// ═══════════════════════════════════════════════════════════════════════

macro_rules! build_app {
    ($ms:expr, $n:expr, $items:expr) => {{
        let pool = create_test_pool();
        { let c = pool.get().unwrap(); run_migrations(&c).unwrap(); }

        let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
        let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
        let batch_repo: Arc<dyn BatchRepo> = Arc::new(SqliteBatchRepo::new(pool.clone()));
        let group_repo: Arc<dyn GroupRepo> = Arc::new(SqliteGroupRepo::new(pool.clone()));
        let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
        let _ = provider_repo.update_api_key("xiaomi", "test-key");
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE providers SET base_url = ?1, is_configured = 1, is_default = 1 WHERE id = 'xiaomi'",
                rusqlite::params![$ms.uri()],
            ).unwrap();
        }

        let sse_bus = Arc::new(SseBus::new());
        let chunker = MimoChunker::new(&$ms.uri(), 2000, 5000);
        let total = $n * $items;
        let cap = (total * 16).max(4096).min(65536);
        let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(cap);

        let client = Arc::new(MimoClient::new(&$ms.uri()));
        let cache_dir = std::path::PathBuf::from(format!("/tmp/concurrent-cache-{}", $n));
        let cache = Arc::new(Cache::new(cache_dir.clone(), Duration::from_secs(3600), 10_000));

        let rl = Arc::new(TokenBucket::new(100_000));
        let tb = Arc::new(TokenBucket::new(10_000_000));
        let prl = Arc::new(ProviderRateLimiterMap::new(100_000, 100_000_000, 100));
        let lb = Arc::new(ProviderLoadBalancer::new());

        let cq = Arc::new(ChunkQueue::new(
            pool.clone(), chunk_repo.clone(), task_repo.clone(),
            client, cache, rl, tb, tx.clone(),
            ($n as usize).min(20).max(4), total.min(500),
            Duration::from_secs(60), cache_dir,
            provider_repo.clone(), prl, lb,
        ));

        let tq = Arc::new(TaskQueue::new(
            pool.clone(), task_repo.clone(), chunk_repo.clone(),
            cq.clone(), group_repo.clone(), tx.clone(), chunker,
        ));

        let ts = Arc::new(TaskService::new(task_repo, chunk_repo, tq.clone(), tx.clone()));
        let gs = Arc::new(GroupService::new(group_repo));
        let bs = Arc::new(BatchService::new(batch_repo, ts.clone(), sse_bus.clone()));

        let state = AppState {
            batch_service: bs, task_service: ts,
            group_service: gs, provider_repo, sse_bus,
        };

        cq.run_workers();
        let tq2 = tq.clone();
        tokio::spawn(async move { tq2.listen(rx).await; });

        test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .app_data(web::JsonConfig::default().limit(50 * 1024 * 1024))
                .configure(um_mimo_tts_server::routes::configure),
        ).await
    }};
}

// ═══════════════════════════════════════════════════════════════════════
// User simulation macro (inline, borrows $app)
// ═══════════════════════════════════════════════════════════════════════

macro_rules! user_flow {
    ($app:expr, $uid:expr, $items:expr, $timeout:expr) => {{
        let uid: usize = $uid;
        let items: usize = $items;
        let timeout: Duration = $timeout;
        let t0 = Instant::now();
        let mut batch_id = String::new();

        // 1. Create batch
        let req = test::TestRequest::post().uri("/api/v2/batches")
            .set_json(&json!({"title": format!("cu-{uid}"), "voice": um_mimo_tts_server::constants::DEFAULT_VOICE}))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        if resp.status() != StatusCode::OK {
            UserMetrics { user_id: uid, batch_id, create_ms: 0, add_items_ms: 0, submit_ms: 0,
                process_ms: 0, total_ms: t0.elapsed().as_millis(),
                ok_count: 0, fail_count: 0, error: Some(format!("create: {}", resp.status())) }
        } else {
            let batch: Value = body_json(resp).await;
            batch_id = batch["id"].as_str().unwrap().to_string();
            let create_ms = t0.elapsed().as_millis();

            // 2. Add items
            let items_json: Vec<Value> = (0..items).map(|i| json!({
                "seq": (i+1) as i32, "filename": format!("u{uid}_f{i}.txt"),
                "content": gen_text(uid * 1000 + i),
            })).collect();
            let req = test::TestRequest::post()
                .uri(&format!("/api/v2/batches/{batch_id}/items/batch"))
                .set_json(&items_json).to_request();
            let resp = test::call_service(&$app, req).await;
            if resp.status() != StatusCode::CREATED {
                UserMetrics { user_id: uid, batch_id, create_ms, add_items_ms: 0, submit_ms: 0,
                    process_ms: 0, total_ms: t0.elapsed().as_millis(),
                    ok_count: 0, fail_count: 0, error: Some(format!("add: {}", resp.status())) }
            } else {
                let add_items_ms = t0.elapsed().as_millis() - create_ms;

                // 3. Submit
                let req = test::TestRequest::post()
                    .uri(&format!("/api/v2/batches/{batch_id}/submit")).to_request();
                let resp = test::call_service(&$app, req).await;
                if resp.status() != StatusCode::OK {
                    UserMetrics { user_id: uid, batch_id, create_ms, add_items_ms, submit_ms: 0,
                        process_ms: 0, total_ms: t0.elapsed().as_millis(),
                        ok_count: 0, fail_count: 0, error: Some(format!("submit: {}", resp.status())) }
                } else {
                    let submit_ms = t0.elapsed().as_millis() - create_ms - add_items_ms;

                    // 4. Poll
                    let deadline = Instant::now() + timeout;
                    let mut result = UserMetrics {
                        user_id: uid, batch_id: batch_id.clone(), create_ms, add_items_ms, submit_ms,
                        process_ms: 0, total_ms: 0, ok_count: 0, fail_count: 0, error: Some("timeout".into()),
                    };
                    loop {
                        if Instant::now() > deadline { break; }
                        let req = test::TestRequest::get()
                            .uri(&format!("/api/v2/batches/{batch_id}")).to_request();
                        let resp = test::call_service(&$app, req).await;
                        let b: Value = body_json(resp).await;
                        let st = b["status"].as_str().unwrap_or_default();
                        if st == "completed" || st == "failed" {
                            let process_ms = t0.elapsed().as_millis() - create_ms - add_items_ms - submit_ms;
                            let mut ok = 0usize; let mut fail = 0usize; let mut pg = 0i64;
                            loop {
                                let req = test::TestRequest::get()
                                    .uri(&format!("/api/v2/tasks?batch_id={batch_id}&page={pg}&page_size=5000"))
                                    .to_request();
                                let resp = test::call_service(&$app, req).await;
                                let tl: Value = body_json(resp).await;
                                let data = tl["data"].as_array().unwrap();
                                if data.is_empty() { break; }
                                for t in data {
                                    match t["status"].as_str().unwrap_or_default() {
                                        "done" => ok += 1, "failed" => fail += 1, _ => {}
                                    }
                                }
                                pg += 1;
                            }
                            result = UserMetrics {
                                user_id: uid, batch_id, create_ms, add_items_ms, submit_ms,
                                process_ms, total_ms: t0.elapsed().as_millis(),
                                ok_count: ok, fail_count: fail,
                                error: if st == "failed" && ok == 0 { Some("batch failed".into()) } else { None },
                            };
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    result
                }
            }
        }
    }};
}

// ═══════════════════════════════════════════════════════════════════════
// Viewer simulation macro
// ═══════════════════════════════════════════════════════════════════════

macro_rules! viewer_flow {
    ($app:expr, $vid:expr, $dur:expr) => {{
        let vid: usize = $vid;
        let dur: Duration = $dur;
        let t0 = Instant::now();
        let deadline = Instant::now() + dur;
        let mut queries = 0u64;
        while Instant::now() < deadline {
            let req = test::TestRequest::get()
                .uri("/api/v2/tasks?page=0&page_size=50").to_request();
            let resp = test::call_service(&$app, req).await;
            assert_eq!(resp.status(), StatusCode::OK);
            let _: Value = body_json(resp).await;
            queries += 1;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        UserMetrics {
            user_id: vid, batch_id: String::new(),
            create_ms: 0, add_items_ms: 0, submit_ms: 0,
            process_ms: t0.elapsed().as_millis(), total_ms: t0.elapsed().as_millis(),
            ok_count: queries as usize, fail_count: 0, error: None,
        }
    }};
}

// ═══════════════════════════════════════════════════════════════════════
// Tests — using tokio::join! for concurrent user simulation
// ═══════════════════════════════════════════════════════════════════════

#[actix_web::test]
async fn concurrent_5_users() {
    let ms = mock_mimo_api().await;
    let app = build_app!(&ms, 5, 10);
    let t0 = Instant::now();
    let sep = "=".repeat(70);

    eprintln!("\n{sep}");
    eprintln!("CONCURRENT TEST: 5 users x 10 items = 50 total");
    eprintln!("{sep}\n");

    // All 5 users run concurrently via tokio::join!
    let (m0, m1, m2, m3, m4) = tokio::join!(
        async { user_flow!(&app, 0, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 1, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 2, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 3, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 4, 10, Duration::from_secs(120)) },
    );

    let wall = t0.elapsed();
    let results = vec![m0, m1, m2, m3, m4];

    for m in &results {
        let err = m.error.as_ref().map(|e| format!(" ERR:{e}")).unwrap_or_default();
        let bid = &m.batch_id;
        let bid_s = if bid.len() > 8 { &bid[..8] } else { bid };
        eprintln!("  user {} | {bid_s} | ok={}/{} | {}ms{err}",
            m.user_id, m.ok_count, m.ok_count + m.fail_count, m.total_ms);
    }

    let r = aggregate(&results, wall);
    print_report(&r);

    assert_eq!(r.user_count, 5);
    assert!(r.error_rate < 0.01, "error rate: {:.1}%", r.error_rate * 100.0);
    assert!(r.p95_ms < 120_000, "p95: {}ms", r.p95_ms);
}

#[actix_web::test]
async fn concurrent_10_users() {
    let ms = mock_mimo_api().await;
    let app = build_app!(&ms, 10, 10);
    let t0 = Instant::now();
    let sep = "=".repeat(70);

    eprintln!("\n{sep}");
    eprintln!("CONCURRENT TEST: 10 users x 10 items = 100 total");
    eprintln!("{sep}\n");

    let (m0, m1, m2, m3, m4, m5, m6, m7, m8, m9) = tokio::join!(
        async { user_flow!(&app, 0, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 1, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 2, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 3, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 4, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 5, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 6, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 7, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 8, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 9, 10, Duration::from_secs(120)) },
    );

    let wall = t0.elapsed();
    let results = vec![m0, m1, m2, m3, m4, m5, m6, m7, m8, m9];
    let ok_users = results.iter().filter(|m| m.error.is_none()).count();
    eprintln!("  {ok_users}/10 users completed without errors");

    let r = aggregate(&results, wall);
    print_report(&r);

    assert_eq!(r.user_count, 10);
    assert!(r.error_rate < 0.05, "error rate: {:.1}%", r.error_rate * 100.0);
    assert!(r.p95_ms < 120_000, "p95: {}ms", r.p95_ms);
}

#[actix_web::test]
async fn mixed_operations() {
    let ms = mock_mimo_api().await;
    let app = build_app!(&ms, 5, 10);
    let t0 = Instant::now();
    let sep = "=".repeat(70);

    eprintln!("\n{sep}");
    eprintln!("MIXED OPERATIONS: 5 creators + 2 viewers");
    eprintln!("{sep}\n");

    // 5 creators + 2 viewers run concurrently
    let (c0, c1, c2, c3, c4, v0, v1) = tokio::join!(
        async { user_flow!(&app, 0, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 1, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 2, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 3, 10, Duration::from_secs(120)) },
        async { user_flow!(&app, 4, 10, Duration::from_secs(120)) },
        async { viewer_flow!(&app, 5, Duration::from_secs(30)) },
        async { viewer_flow!(&app, 6, Duration::from_secs(30)) },
    );

    let creators = vec![c0, c1, c2, c3, c4];
    let viewers = vec![v0, v1];

    let ce: usize = creators.iter().filter(|m| m.error.is_some()).count();
    let ve: usize = viewers.iter().filter(|m| m.error.is_some()).count();
    let total_queries: usize = viewers.iter().map(|m| m.ok_count).sum();

    eprintln!("  creators: {}/5 ok (errors={ce})", 5 - ce);
    eprintln!("  viewers: {}/2 ok (errors={ve}), total queries={total_queries}", 2 - ve);

    assert_eq!(creators.len(), 5);
    assert_eq!(ce, 0, "creator errors");
    assert_eq!(viewers.len(), 2);
    assert_eq!(ve, 0, "viewer errors");

    let r = aggregate(&creators, t0.elapsed());
    print_report(&r);
}
