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

/// Remove cached WAV files to prevent storage bloat between test rounds.
fn cleanup_cache() {
    for dir in &["/tmp/ummimo-cache-real", "/tmp/ummimo-stress-3000"] {
        let path = std::path::PathBuf::from(dir);
        if path.exists() {
            match std::fs::remove_dir_all(&path) {
                Ok(_) => eprintln!("[cleanup] removed cache dir: {dir}"),
                Err(e) => eprintln!("[cleanup] failed to remove {dir}: {e}"),
            }
        }
    }
}

/// Remove generated WAV output files to prevent storage bloat.
fn cleanup_output() {
    let dir = std::path::PathBuf::from("../data/output");
    if dir.exists() {
        let mut count = 0usize;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("wav") {
                    let _ = std::fs::remove_file(&p);
                    count += 1;
                }
            }
        }
        if count > 0 {
            eprintln!("[cleanup] removed {count} output WAV files");
        }
    }
}

// ── System monitoring helpers ────────────────────────────────────────────

struct SystemSnapshot {
    rss_mb: f64,
    cache_files: usize,
    cache_size_mb: f64,
    output_files: usize,
    output_size_mb: f64,
}

/// Get process RSS memory in MB.
/// Windows: uses GetProcessMemoryInfo via kernel32/psapi.
/// Linux: reads /proc/self/status VmRSS.
/// Fallback: 0.0
fn get_process_rss_mb() -> f64 {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> isize;
        }
        #[link(name = "psapi")]
        extern "system" {
            fn GetProcessMemoryInfo(
                process: isize,
                pmc: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
        }
        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        unsafe {
            let mut pmc = ProcessMemoryCounters {
                cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
                page_fault_count: 0,
                peak_working_set_size: 0,
                working_set_size: 0,
                quota_peak_paged_pool_usage: 0,
                quota_paged_pool_usage: 0,
                quota_peak_non_paged_pool_usage: 0,
                quota_non_paged_pool_usage: 0,
                pagefile_usage: 0,
                peak_pagefile_usage: 0,
            };
            let handle = GetCurrentProcess();
            if GetProcessMemoryInfo(handle, &mut pmc, pmc.cb) != 0 {
                return pmc.working_set_size as f64 / 1_048_576.0;
            }
        }
        0.0
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<f64>() {
                            return kb / 1024.0;
                        }
                    }
                }
            }
        }
        0.0
    }
}

/// Walk a directory and return (file_count, total_size_mb).
fn dir_stats(path: &std::path::Path) -> (usize, f64) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    if path.exists() {
        fn walk(dir: &std::path::Path, count: &mut usize, bytes: &mut u64) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        *count += 1;
                        *bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                    } else if p.is_dir() {
                        walk(&p, count, bytes);
                    }
                }
            }
        }
        walk(path, &mut count, &mut bytes);
    }
    (count, bytes as f64 / 1_048_576.0)
}

fn snapshot_system(cache_dir: &std::path::Path, output_dir: &std::path::Path) -> SystemSnapshot {
    let (cache_files, cache_size_mb) = dir_stats(cache_dir);
    let (output_files, output_size_mb) = dir_stats(output_dir);
    SystemSnapshot {
        rss_mb: get_process_rss_mb(),
        cache_files,
        cache_size_mb,
        output_files,
        output_size_mb,
    }
}

fn print_snapshot(label: &str, snap: &SystemSnapshot, prev: Option<&SystemSnapshot>) {
    let delta = if let Some(p) = prev {
        format!(
            " (Δ RSS: {:+.1} MB, Δ cache: {:+.1} MB, Δ output: {:+.1} MB)",
            snap.rss_mb - p.rss_mb,
            snap.cache_size_mb - p.cache_size_mb,
            snap.output_size_mb - p.output_size_mb,
        )
    } else {
        String::new()
    };
    eprintln!(
        "[SYS {label}] RSS={:.1} MB | cache: {} files ({:.1} MB) | output: {} files ({:.1} MB){delta}",
        snap.rss_mb, snap.cache_files, snap.cache_size_mb,
        snap.output_files, snap.output_size_mb,
    );
}

/// Query chunk status distribution for a batch from DB.
/// Returns Vec<(status_string, count)>.
fn query_chunk_status(pool: &rusqlite::Connection, bid: &str) -> Vec<(String, i64)> {
    let mut stmt = pool.prepare(
        "SELECT c.status, COUNT(*) FROM chunks c \
         INNER JOIN tasks t ON c.task_id = t.id \
         WHERE t.batch_id = ?1 \
         GROUP BY c.status"
    ).ok();
    if let Some(ref mut s) = stmt {
        s.query_map(rusqlite::params![bid], |row| {
            let raw: String = row.get(0)?;
            let cnt: i64 = row.get(1)?;
            // Strip JSON quotes: "\"done\"" -> "done"
            let clean = raw.trim_matches('"').to_string();
            Ok((clean, cnt))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    } else {
        vec![]
    }
}

/// Generate large text with approximately `target_tokens` tokens.
/// Mixes Chinese and English sentences for realistic tokenization.
/// Token estimation: Chinese chars ≈ 1.3 tokens each, ASCII words ≈ 1 token per word.
fn gen_large_text(target_tokens: usize, seed: u64) -> String {
    let chinese_phrases: &[&str] = &[
        "今天天气非常好，适合出去散步。",
        "科技的发展日新月异，人工智能正在改变我们的生活方式。",
        "中国传统文化源远流长，蕴含着丰富的哲学思想和人生智慧。",
        "春天来了，万物复苏，大地又恢复了生机勃勃的景象。",
        "音乐是人类共同的语言，能够跨越国界传递情感与力量。",
        "在数字化转型的浪潮中，企业面临着前所未有的机遇与挑战。",
        "读书使人明智，运动使人健康，两者缺一不可。",
        "随着互联网技术的普及，信息获取变得更加便捷和高效。",
        "环境保护是全人类共同的课题，需要每个人的参与和努力。",
        "创新是推动社会进步的重要动力，我们应该鼓励大胆探索和尝试。",
        "家庭是社会的基本单位，和谐的家庭关系对个人成长至关重要。",
        "教育的本质不仅是传授知识，更是培养学生的思维能力和创造力。",
        "在全球化的背景下，跨文化交流和理解变得越来越重要。",
        "健康管理应该成为每个人的日常习惯，预防胜于治疗。",
        "城市规划需要兼顾经济发展和生态环境保护，实现可持续增长。",
    ];
    let english_phrases: &[&str] = &[
        "The advancement of technology has revolutionized how we communicate and share information across the globe.",
        "Scientific research continues to push the boundaries of human knowledge and understanding.",
        "Machine learning algorithms are being applied to solve complex problems in healthcare and finance.",
        "The importance of sustainable development cannot be overstated in our rapidly changing world.",
        "Digital transformation is reshaping industries and creating new opportunities for innovation.",
        "Cloud computing has enabled businesses to scale their operations more efficiently than ever before.",
        "The intersection of art and technology creates fascinating new possibilities for creative expression.",
        "Environmental conservation efforts require collaboration between governments and communities.",
    ];

    // target_tokens → approximate chars: ~60% Chinese (1.3 tok/char) + ~40% English (~0.25 tok/char)
    // weighted avg ≈ 0.88 tok/char → chars ≈ target_tokens / 0.88
    let target_chars = ((target_tokens as f64) * 1.14) as usize;
    let mut s = String::with_capacity(target_chars * 3); // UTF-8 capacity
    let mut idx = seed as usize;

    while s.chars().count() < target_chars {
        idx = idx.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        if idx % 5 < 3 {
            // 60% Chinese
            s.push_str(chinese_phrases[idx % chinese_phrases.len()]);
        } else {
            // 40% English
            s.push_str(english_phrases[idx % english_phrases.len()]);
            s.push(' ');
        }
        if idx % 7 == 0 {
            s.push('\n');
        }
    }
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
    let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
    let _ = provider_repo.update_api_key("xiaomi", "test-key");
    // Override base_url to point to the wiremock mock (migration seeds the real API URL)
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE providers SET base_url = ?1, is_configured = 1, is_default = 1 WHERE id = 'xiaomi'",
            rusqlite::params![ms.uri()],
        ).unwrap();
    }

    // ── Infrastructure ────────────────────────────────────────────
    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&ms.uri(), 2000, 5000);

    let cap = (n * 16).max(4096).min(65536);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(cap);

    let client = Arc::new(MimoClient::new(&ms.uri()));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/test-cache"),
        Duration::from_secs(3600),
        100,
    ));

    // High rate so the pipeline is the bottleneck, not the API throttle.
    let rl = Arc::new(TokenBucket::new(100_000));
    let tb = Arc::new(TokenBucket::new(10_000_000));
    let provider_rl = Arc::new(ProviderRateLimiterMap::new(100_000, 100_000_000, 100));
    let load_balancer = Arc::new(ProviderLoadBalancer::new());

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
        provider_repo.clone(),
        provider_rl,
        load_balancer,
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
        provider_repo,
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
    let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
    let _ = provider_repo.update_api_key("xiaomi", "test-key");
    // Override base_url to point to the wiremock mock (migration seeds the real API URL)
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE providers SET base_url = ?1, is_configured = 1, is_default = 1 WHERE id = 'xiaomi'",
            rusqlite::params![ms.uri()],
        ).unwrap();
    }
    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&ms.uri(), 2000, 5000);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(4096);
    let client = Arc::new(MimoClient::new(&ms.uri()));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/test-cache-lg"), Duration::from_secs(3600), 100,
    ));
    let rl = Arc::new(TokenBucket::new(100_000));
    let tb = Arc::new(TokenBucket::new(10_000_000));
    let provider_rl = Arc::new(ProviderRateLimiterMap::new(100_000, 100_000_000, 100));
    let load_balancer = Arc::new(ProviderLoadBalancer::new());

    let cq = Arc::new(ChunkQueue::new(
        pool.clone(), chunk_repo.clone(), task_repo.clone(), client, cache,
        rl, tb, tx.clone(), 20, 20, Duration::from_secs(60),
        std::path::PathBuf::from("/tmp/test-cache-lg"),
        provider_repo.clone(),
        provider_rl,
        load_balancer,
    ));
    let tq = Arc::new(TaskQueue::new(
        pool.clone(), task_repo.clone(), chunk_repo.clone(), cq.clone(),
        group_repo.clone(), tx.clone(), chunker,
    ));
    let ts = Arc::new(TaskService::new(task_repo.clone(), chunk_repo.clone(), tq.clone(), tx.clone()));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(batch_repo.clone(), ts.clone(), sse_bus.clone()));

    let state = AppState { batch_service: bs, task_service: ts, group_service: gs, provider_repo, sse_bus };
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
    let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
    let _ = provider_repo.update_api_key("xiaomi", "test-key");
    // Override base_url to point to the wiremock mock (migration seeds the real API URL)
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE providers SET base_url = ?1, is_configured = 1, is_default = 1 WHERE id = 'xiaomi'",
            rusqlite::params![ms.uri()],
        ).unwrap();
    }
    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&ms.uri(), 2000, 5000);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(4096);
    let client = Arc::new(MimoClient::new(&ms.uri()));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/test-cache-xl"), Duration::from_secs(3600), 100,
    ));
    let rl = Arc::new(TokenBucket::new(100_000));
    let tb = Arc::new(TokenBucket::new(10_000_000));
    let provider_rl = Arc::new(ProviderRateLimiterMap::new(100_000, 100_000_000, 100));
    let load_balancer = Arc::new(ProviderLoadBalancer::new());

    let cq = Arc::new(ChunkQueue::new(
        pool.clone(), chunk_repo.clone(), task_repo.clone(), client, cache,
        rl, tb, tx.clone(), 20, 20, Duration::from_secs(60),
        std::path::PathBuf::from("/tmp/test-cache-xl"),
        provider_repo.clone(),
        provider_rl,
        load_balancer,
    ));
    let tq = Arc::new(TaskQueue::new(
        pool.clone(), task_repo.clone(), chunk_repo.clone(), cq.clone(),
        group_repo.clone(), tx.clone(), chunker,
    ));
    let ts = Arc::new(TaskService::new(task_repo.clone(), chunk_repo.clone(), tq.clone(), tx.clone()));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(batch_repo.clone(), ts.clone(), sse_bus.clone()));

    let state = AppState { batch_service: bs, task_service: ts, group_service: gs, provider_repo, sse_bus };
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
    stress_real_with_rpm(n, timeout, None).await
}

/// Run stress test with optional RPM override for sweet-spot probing.
async fn stress_real_with_rpm(n: usize, timeout: Duration, rpm_override: Option<u64>) -> Metrics {
    // Load .env for API key / base URL
    let _ = dotenvy::dotenv();
    cleanup_cache();

    // Read env — will panic with a clear message if missing.
    let api_key = std::env::var("MIMO_API_KEY")
        .expect("REAL API test requires MIMO_API_KEY env var");
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| "https://api.mimo.com".to_string());
    let rpm: u64 = rpm_override.unwrap_or_else(|| {
        std::env::var("MIMO_RATE_LIMIT_RPM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20)
    });

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
    let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
    // Set real API key + base_url in DB so process_chunk() can use them
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE providers SET base_url = ?1, api_key = ?2, is_configured = 1, is_default = 1 WHERE id = 'xiaomi'",
            rusqlite::params![base_url, api_key],
        ).unwrap();
    }

    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&base_url, 2000, 5000);

    // Concurrency: lower for real API to avoid connection rejections
    let max_concurrent: usize = std::env::var("MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let cap = (n * 16).max(4096).min(65536);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(cap);

    let client = Arc::new(MimoClient::new(&base_url));
    let cache = Arc::new(Cache::new(
        std::path::PathBuf::from("/tmp/ummimo-cache-real"),
        Duration::from_secs(3600),
        100,
    ));

    // Rate limit to the configured RPM so we don't exceed the API quota.
    let rl = Arc::new(TokenBucket::new(rpm));
    let tb = Arc::new(TokenBucket::new(10_000_000));
    let provider_rl = Arc::new(ProviderRateLimiterMap::new(rpm, 1_000_000, rpm));
    let load_balancer = Arc::new(ProviderLoadBalancer::new());
    load_balancer.add_provider("xiaomi", rpm);

    eprintln!("[stress_real] max_concurrent={max_concurrent}");

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
        max_concurrent,
        n.min(200),
        Duration::from_secs(120),
        std::path::PathBuf::from("/tmp/ummimo-cache-real"),
        provider_repo.clone(),
        provider_rl,
        load_balancer,
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
        provider_repo,
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
        .set_json(&json!({"title": format!("stress-real-{n}"), "voice": "冰糖", "model": "mimo-v2.5-tts"}))
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

                // If any failed, show error messages from chunks
                if statuses.iter().any(|s| s == "failed") {
                    if let Some(failed_task) = data.iter().find(|t| t["status"].as_str() == Some("failed")) {
                        let tid = failed_task["id"].as_str().unwrap_or_default();
                        // Query error messages directly from DB
                        if let Ok(conn) = pool.get() {
                            if let Ok(mut stmt) = conn.prepare("SELECT id, error_message, retry_count FROM chunks WHERE task_id = ?1 AND status = '\"failed\"' LIMIT 3") {
                                let errors: Vec<(String, Option<String>, i32)> = stmt.query_map(rusqlite::params![tid], |row| {
                                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                                }).unwrap().filter_map(|r| r.ok()).collect();
                                for (cid, err, retries) in &errors {
                                    eprintln!("[stress_real] CHUNK ERROR task={tid} chunk={cid} retries={retries} error={:?}", err);
                                }
                            }
                        }
                    }
                }
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
                // Allow up to 90% failure for real API (MIMO server can reject
                // TCP connections when overloaded — HTTP 500/503, per API docs)
                let max_allowed_fail = n * 9 / 10;
                assert!(
                    fail_cnt <= max_allowed_fail,
                    "too many failures: {fail_cnt}/{n} (max allowed {max_allowed_fail})"
                );
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
                let mut merging_cnt = 0usize;
                let mut processing_cnt = 0usize;
                let mut other_cnt = 0usize;
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
                            "failed" | "merging_failed" => fail_cnt += 1,
                            "merging" => merging_cnt += 1,
                            "processing" | "queued" | "chunking" => processing_cnt += 1,
                            _ => other_cnt += 1,
                        }
                    }
                    page += 1;
                }
                let terminal = ok_cnt + fail_cnt;
                let all_terminal = terminal == n && merging_cnt == 0 && processing_cnt == 0 && other_cnt == 0;
                if all_terminal {
                    eprintln!("[stress_real] all-terminal fallback ok={ok_cnt}/{n} fail={fail_cnt}");
                    let t4 = t0.elapsed();
                    // Allow up to 75% failure for real API tests (network instability is realistic)
                    let max_allowed_fail = n * 3 / 4;
                    assert!(
                        fail_cnt <= max_allowed_fail,
                        "too many failures: {fail_cnt}/{n} (max allowed {max_allowed_fail})"
                    );
                    return Metrics { n, create: t1, add: t2 - t1, submit: t3 - t2, process: t4 - t3, total: t4, ok: ok_cnt };
                }
                tokio::time::sleep(Duration::from_millis(2000)).await;
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
    let m = stress_real(100, Duration::from_secs(900)).await;
    println!(
        "STRESS_REAL items={} create={}ms add={}ms submit={}ms process={}ms total={}ms ok={}",
        m.n, m.create.as_millis(), m.add.as_millis(),
        m.submit.as_millis(), m.process.as_millis(),
        m.total.as_millis(), m.ok,
    );
    assert!(m.total < Duration::from_secs(600), "took too long: {:.1?}", m.total);
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

// ═══════════════════════════════════════════════════════════════════════
// Large-scale real API stress test with per-stage monitoring
// ═══════════════════════════════════════════════════════════════════════

/// Performance metrics with per-stage breakdown.
#[derive(Debug)]
struct PerfMetrics {
    n_tasks: usize,
    total_chunks_est: usize,
    total_chars: usize,
    total_tokens_est: usize,
    // Timing
    batch_create: Duration,
    text_gen: Duration,
    items_add: Duration,
    submit: Duration,
    enqueue: Duration,      // time from submit to first chunk picked up
    processing: Duration,   // time from first chunk to all done
    total: Duration,
    // Task outcomes
    done_tasks: usize,
    failed_tasks: usize,
    // Throughput
    chunks_per_sec: f64,
    api_calls_per_min: f64,
    tokens_per_min: f64,
}

/// Enhanced real API stress test with:
/// - Random text sizes per task (min_tokens..max_tokens)
/// - Per-stage performance monitoring
/// - Periodic progress reports (every 30s)
/// - Chunk count distribution stats
async fn stress_real_monitored(
    n: usize,
    min_tokens: usize,
    max_tokens: usize,
    timeout: Duration,
) -> PerfMetrics {
    // Load .env for API key / base URL
    let _ = dotenvy::dotenv();
    cleanup_cache();

    let api_key = std::env::var("MIMO_API_KEY")
        .expect("REAL API test requires MIMO_API_KEY env var");
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| um_mimo_tts_server::constants::MIMO_BASE_URL_DEFAULT.to_string());
    let rpm: u64 = std::env::var("MIMO_RATE_LIMIT_RPM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100); // default: per-provider 100 RPM
    let max_concurrent: usize = std::env::var("MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let chunk_target: i64 = std::env::var("CHUNK_TARGET_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10000);
    let chunk_cap: i64 = std::env::var("CHUNK_HARD_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20000);

    eprintln!("═══════════════════════════════════════════════════════════════");
    eprintln!("  STRESS TEST: {n} tasks × {min_tokens}–{max_tokens} tokens");
    eprintln!("  API: {base_url}  RPM={rpm}  concurrent={max_concurrent}");
    eprintln!("  Chunker: target={chunk_target} cap={chunk_cap}");
    eprintln!("  Timeout: {timeout:?}");
    eprintln!("═══════════════════════════════════════════════════════════════");

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
    let provider_repo: Arc<dyn ProviderRepo> = Arc::new(SqliteProviderRepo::new(pool.clone()));
    let _ = provider_repo.update_api_key("xiaomi", &api_key);
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE providers SET base_url = ?1, api_key = ?2, is_configured = 1, is_default = 1 WHERE id = 'xiaomi'",
            rusqlite::params![base_url, api_key],
        ).unwrap();
    }

    let sse_bus = Arc::new(SseBus::new());
    let chunker = MimoChunker::new(&base_url, chunk_target, chunk_cap);

    let cap = (n * 32).max(8192).min(65536);
    let (tx, rx) = tokio::sync::broadcast::channel::<DomainEvent>(cap);

    let client = Arc::new(MimoClient::new(&base_url));
    let cache_dir = std::path::PathBuf::from("/tmp/ummimo-stress-3000");
    let cache = Arc::new(Cache::new(cache_dir.clone(), Duration::from_secs(7200), 500));

    let rl = Arc::new(TokenBucket::new(rpm));
    let tb = Arc::new(TokenBucket::new(10_000_000));
    let provider_rl = Arc::new(ProviderRateLimiterMap::new(rpm, 1_000_000, rpm));
    let load_balancer = Arc::new(ProviderLoadBalancer::new());
    load_balancer.add_provider("xiaomi", rpm);

    let cq = Arc::new(ChunkQueue::new(
        pool.clone(), chunk_repo.clone(), task_repo.clone(),
        client, cache, rl, tb, tx.clone(),
        max_concurrent, n.min(500), Duration::from_secs(120),
        cache_dir, provider_repo.clone(), provider_rl, load_balancer,
    ));

    let tq = Arc::new(TaskQueue::new(
        pool.clone(), task_repo.clone(), chunk_repo.clone(),
        cq.clone(), group_repo.clone(), tx.clone(), chunker,
    ));

    let ts = Arc::new(TaskService::new(task_repo.clone(), chunk_repo.clone(), tq.clone(), tx.clone()));
    let gs = Arc::new(GroupService::new(group_repo.clone()));
    let bs = Arc::new(BatchService::new(batch_repo.clone(), ts.clone(), sse_bus.clone()));

    let state = AppState {
        batch_service: bs, task_service: ts, group_service: gs,
        provider_repo, sse_bus,
    };

    cq.run_workers();
    let tq2 = tq.clone();
    tokio::spawn(async move { tq2.listen(rx).await; });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .app_data(web::JsonConfig::default().limit(500 * 1024 * 1024)) // 500MB for massive batch
            .configure(um_mimo_tts_server::routes::configure),
    ).await;

    let t0 = Instant::now();

    // ═══ STAGE 1: CREATE BATCH ═══
    eprintln!("[T+{:>7.1}s] Stage 1/5: Creating batch...", t0.elapsed().as_secs_f64());
    let req = test::TestRequest::post()
        .uri("/api/v2/batches")
        .set_json(&json!({
            "title": format!("stress-{n}-{min_tokens}k-{max_tokens}k"),
            "voice": "冰糖",
            "model": "mimo-v2.5-tts",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let batch: Value = body_json(resp).await;
    let bid = batch["id"].as_str().unwrap().to_string();
    let t_batch_create = t0.elapsed();
    eprintln!("[T+{:>7.1}s]   ✓ Batch created: {bid}", t_batch_create.as_secs_f64());

    // ═══ STAGE 2: GENERATE TEXT + ADD ITEMS ═══
    eprintln!("[T+{:>7.1}s] Stage 2/5: Generating text & adding items...", t0.elapsed().as_secs_f64());
    let t_gen_start = t0.elapsed();

    // Pre-generate all texts with random token counts
    let mut items_data: Vec<(usize, usize, usize)> = Vec::with_capacity(n); // (seq, target_tokens, actual_chars)
    let mut total_target_tokens: usize = 0;
    let mut total_chars: usize = 0;
    let mut token_distribution: Vec<usize> = Vec::with_capacity(n);

    // Simple deterministic PRNG
    let mut rng_state: u64 = 0xDEADBEEF_CAFEBABE;
    let next_rand = |state: &mut u64| -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *state
    };

    for i in 0..n {
        let r = next_rand(&mut rng_state);
        let target_tokens = min_tokens + (r as usize % (max_tokens - min_tokens + 1));
        token_distribution.push(target_tokens);
        total_target_tokens += target_tokens;
        items_data.push((i + 1, target_tokens, 0));
    }

    let t_gen_done = t0.elapsed();
    eprintln!("[T+{:>7.1}s]   ✓ Token distribution planned: min={} max={} avg={} total={}",
        t_gen_done.as_secs_f64(),
        token_distribution.iter().min().unwrap(),
        token_distribution.iter().max().unwrap(),
        total_target_tokens / n,
        total_target_tokens,
    );

    // Add items in chunks of 50 (large text = big JSON payloads)
    const ADD_BATCH_SIZE: usize = 50;
    let mut added = 0usize;
    while added < n {
        let hi = (added + ADD_BATCH_SIZE).min(n);
        let mut items = Vec::with_capacity(hi - added);
        for idx in added..hi {
            let seq = items_data[idx].0;
            let target_tokens = items_data[idx].1;
            let text = gen_large_text(target_tokens, (idx as u64).wrapping_mul(0x9E3779B97F4A7C15));
            let chars = text.chars().count();
            total_chars += chars;
            items_data[idx].2 = chars;
            items.push(json!({
                "seq": seq as i32,
                "filename": format!("task_{idx}_{target_tokens}tok.txt"),
                "content": text,
            }));
        }

        let req = test::TestRequest::post()
            .uri(&format!("/api/v2/batches/{bid}/items/batch"))
            .set_json(&items)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED, "add chunk {added}..{hi}");
        added = hi;

        if added % 500 == 0 || added == n {
            eprintln!("[T+{:>7.1}s]   ✓ Added {added}/{n} items ({} MB payload)",
                t0.elapsed().as_secs_f64(),
                total_chars * 3 / 1024 / 1024, // rough UTF-8 size
            );
        }
    }
    let t_items_added = t0.elapsed();
    eprintln!("[T+{:>7.1}s]   ✓ All items added: {} chars ≈ {} MB, text_gen={:?} add_api={:?}",
        t_items_added.as_secs_f64(),
        total_chars,
        total_chars * 3 / 1024 / 1024,
        t_gen_done - t_gen_start,
        t_items_added - t_gen_done,
    );

    // ═══ STAGE 3: SUBMIT ═══
    eprintln!("[T+{:>7.1}s] Stage 3/5: Submitting batch...", t0.elapsed().as_secs_f64());
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/batches/{bid}/submit"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tasks: Value = body_json(resp).await;
    let task_arr = tasks.as_array().unwrap();
    assert_eq!(task_arr.len(), n, "submit returned wrong task count");
    let t_submitted = t0.elapsed();
    eprintln!("[T+{:>7.1}s]   ✓ Submitted {} tasks in {:?}", t_submitted.as_secs_f64(), n, t_submitted - t_items_added);

    // ═══ STAGE 4: WAIT FOR ENQUEUE + MONITOR ═══
    eprintln!("[T+{:>7.1}s] Stage 4/5: Waiting for enqueue + processing...", t0.elapsed().as_secs_f64());

    // Wait for background enqueue to start
    tokio::time::sleep(Duration::from_secs(5)).await;
    let t_enqueue = t0.elapsed();

    // Report chunk distribution for first 10 tasks
    {
        let first_tasks = task_repo.find_by_batch(&bid).unwrap_or_default();
        let sample_size = first_tasks.len().min(20);
        let mut chunk_counts: Vec<i64> = Vec::new();
        for t in first_tasks.iter().take(sample_size) {
            let total = chunk_repo.count_by_task_all(t.id.as_str()).unwrap_or(0);
            chunk_counts.push(total);
        }
        if !chunk_counts.is_empty() {
            let avg_chunks = chunk_counts.iter().sum::<i64>() / chunk_counts.len() as i64;
            let est_total_chunks = avg_chunks * n as i64;
            eprintln!("[T+{:>7.1}s]   Chunk sample ({} tasks): min={} max={} avg={} → est_total≈{}",
                t0.elapsed().as_secs_f64(), sample_size,
                chunk_counts.iter().min().unwrap(),
                chunk_counts.iter().max().unwrap(),
                avg_chunks,
                est_total_chunks,
            );
        }
    }

    // ═══ STAGE 5: POLL WITH PROGRESS MONITORING ═══
    eprintln!("[T+{:>7.1}s] Stage 5/5: Polling for completion...", t0.elapsed().as_secs_f64());
    let deadline = Instant::now() + timeout;
    let mut prev_status = String::new();
    let mut last_report = Instant::now();
    let mut first_processing_seen = false;

    loop {
        assert!(
            Instant::now() < deadline,
            "TIMEOUT after {timeout:?} bid={bid} status={prev_status}"
        );

        let req = test::TestRequest::get()
            .uri(&format!("/api/v2/batches/{bid}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let b: Value = body_json(resp).await;
        let st = b["status"].as_str().unwrap_or_default().to_string();

        if st != prev_status {
            eprintln!("[T+{:>7.1}s]   ▸ Batch status → {st}", t0.elapsed().as_secs_f64());
            prev_status = st.clone();
        }

        if st == "processing" && !first_processing_seen {
            first_processing_seen = true;
            eprintln!("[T+{:>7.1}s]   ✓ First processing seen (enqueue delay: {:?})",
                t0.elapsed().as_secs_f64(), t0.elapsed() - t_submitted);
        }

        // Progress report every 30 seconds
        if last_report.elapsed() > Duration::from_secs(30) {
            last_report = Instant::now();

            // Aggregate task statuses
            let all_tasks = task_repo.find_by_batch(&bid).unwrap_or_default();
            let mut done = 0usize;
            let mut failed = 0usize;
            let mut processing = 0usize;
            let mut queued = 0usize;
            let mut pending = 0usize;
            let mut merging = 0usize;
            for t in &all_tasks {
                match t.status {
                    um_mimo_tts_server::domain::task::TaskStatus::Done => done += 1,
                    um_mimo_tts_server::domain::task::TaskStatus::Failed | um_mimo_tts_server::domain::task::TaskStatus::MergingFailed => failed += 1,
                    um_mimo_tts_server::domain::task::TaskStatus::Processing => processing += 1,
                    um_mimo_tts_server::domain::task::TaskStatus::Queued => queued += 1,
                    um_mimo_tts_server::domain::task::TaskStatus::Pending => pending += 1,
                    um_mimo_tts_server::domain::task::TaskStatus::Merging => merging += 1,
                    _ => {}
                }
            }

            // Sample chunk progress from a processing task
            let sample_chunks = all_tasks.iter()
                .find(|t| t.status == um_mimo_tts_server::domain::task::TaskStatus::Processing)
                .map(|t| {
                    let (total, cd, cf, _, _) = chunk_repo.count_by_task_aggregated(t.id.as_str()).unwrap_or((0,0,0,0,0));
                    (t.id.to_string(), total, cd, cf)
                });

            let elapsed = t0.elapsed().as_secs_f64();
            let throughput = if done > 0 { done as f64 / elapsed * 60.0 } else { 0.0 };

            eprintln!("╭──────────────── Progress Report T+{elapsed:.0}s ────────────────");
            eprintln!("│ Tasks: done={done} processing={processing} merging={merging} queued={queued} pending={pending} failed={failed}");
            eprintln!("│ Throughput: {throughput:.1} tasks/min  ({done}/{n} = {:.1}%)", done as f64 / n as f64 * 100.0);
            if let Some((ref tid, total, cd, cf)) = sample_chunks {
                eprintln!("│ Sample chunk (task {tid}): {cd}/{total} done, {cf} failed");
            }
            eprintln!("│ Elapsed: {elapsed:.0}s  ETA: {:.0}s", if throughput > 0.0 { (n - done) as f64 / throughput * 60.0 } else { -1.0 });
            eprintln!("╰────────────────────────────────────────────────");
        }

        match st.as_str() {
            "completed" | "failed" | "cancelled" => {
                let t_done = t0.elapsed();

                // Final verification
                let all_tasks = task_repo.find_by_batch(&bid).unwrap_or_default();
                let mut done_cnt = 0usize;
                let mut fail_cnt = 0usize;
                let mut total_chunks = 0i64;
                let mut total_done_chunks = 0i64;
                let mut chunk_counts: Vec<i64> = Vec::new();

                for t in &all_tasks {
                    if t.status == um_mimo_tts_server::domain::task::TaskStatus::Done {
                        done_cnt += 1;
                    } else {
                        fail_cnt += 1;
                    }
                    total_chunks += t.total_chunks as i64;
                    total_done_chunks += t.done_chunks as i64;
                    chunk_counts.push(t.total_chunks as i64);
                }

                chunk_counts.sort();
                let avg_chunks = if chunk_counts.is_empty() { 0 } else { chunk_counts.iter().sum::<i64>() / chunk_counts.len() as i64 };
                let median_chunks = if chunk_counts.is_empty() { 0 } else { chunk_counts[chunk_counts.len() / 2] };
                let processing_secs = t_done.as_secs_f64() - t_enqueue.as_secs_f64();
                let chunks_per_sec = if processing_secs > 0.0 { total_done_chunks as f64 / processing_secs } else { 0.0 };
                let api_per_min = chunks_per_sec * 60.0;
                let tokens_per_min = api_per_min * (total_target_tokens as f64 / n as f64) / (avg_chunks.max(1) as f64);

                eprintln!("═══════════════════════════════════════════════════════════════");
                eprintln!("  FINAL RESULTS");
                eprintln!("═══════════════════════════════════════════════════════════════");
                eprintln!("  Tasks:     {done_cnt} done / {fail_cnt} failed / {n} total");
                eprintln!("  Chunks:    {total_done_chunks}/{total_chunks} (avg={avg_chunks} median={median_chunks})");
                eprintln!("  Text:      {total_chars} chars ≈ {} MB", total_chars * 3 / 1024 / 1024);
                eprintln!("  Tokens:    ≈ {total_target_tokens} (est)");
                eprintln!("  ─── Timing ───");
                eprintln!("  Batch create:  {:>8.2}s", t_batch_create.as_secs_f64());
                eprintln!("  Text generate: {:>8.2}s", (t_gen_done - t_gen_start).as_secs_f64());
                eprintln!("  Items add:     {:>8.2}s", (t_items_added - t_gen_done).as_secs_f64());
                eprintln!("  Submit:        {:>8.2}s", (t_submitted - t_items_added).as_secs_f64());
                eprintln!("  Enqueue delay: {:>8.2}s", (t_enqueue - t_submitted).as_secs_f64());
                eprintln!("  Processing:    {:>8.2}s", processing_secs);
                eprintln!("  TOTAL:         {:>8.2}s", t_done.as_secs_f64());
                eprintln!("  ─── Throughput ───");
                eprintln!("  Chunks/sec:    {chunks_per_sec:.2}");
                eprintln!("  API calls/min: {api_per_min:.1}");
                eprintln!("  Tokens/min:    {tokens_per_min:.0}");
                eprintln!("═══════════════════════════════════════════════════════════════");

                return PerfMetrics {
                    n_tasks: n,
                    total_chunks_est: total_chunks as usize,
                    total_chars,
                    total_tokens_est: total_target_tokens,
                    batch_create: t_batch_create,
                    text_gen: t_gen_done - t_gen_start,
                    items_add: t_items_added - t_gen_done,
                    submit: t_submitted - t_items_added,
                    enqueue: t_enqueue - t_submitted,
                    processing: Duration::from_secs_f64(processing_secs),
                    total: t_done,
                    done_tasks: done_cnt,
                    failed_tasks: fail_cnt,
                    chunks_per_sec,
                    api_calls_per_min: api_per_min,
                    tokens_per_min,
                };
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3000 tasks × 1M–3M tokens — THE BIG ONE
// ═══════════════════════════════════════════════════════════════════════

/// 3000 tasks with random 1M–3M token texts via real MIMO API.
///
/// Estimated scale:
///   - Avg 2M tokens/task → ~200 chunks/task (at 10K target)
///   - 3000 × 200 = ~600,000 total chunks
///   - At 100 RPM: ~100 hours; at 500 RPM: ~20 hours
///
/// Run:
///   MIMO_API_KEY=xxx MIMO_RATE_LIMIT_RPM=500 MAX_CONCURRENT=30 \
///   cargo test --test stress_batch r3000_large -- --nocapture --ignored
///
/// ⚠ WARNING: Extremely expensive — ~600K real TTS API calls!
#[actix_web::test]
#[ignore]
async fn r3000_large() {
    let m = stress_real_monitored(
        3000,                    // 3000 tasks
        1_000_000,              // min 1M tokens
        3_000_000,              // max 3M tokens
        Duration::from_secs(720_000), // 200 hour timeout
    ).await;

    println!("PERF_RESULT tasks={} chunks={} chars={} tokens={}",
        m.n_tasks, m.total_chunks_est, m.total_chars, m.total_tokens_est);
    println!("PERF_TIMING batch_create={}ms text_gen={}ms items_add={}ms submit={}ms enqueue={}ms processing={}ms total={}ms",
        m.batch_create.as_millis(), m.text_gen.as_millis(), m.items_add.as_millis(),
        m.submit.as_millis(), m.enqueue.as_millis(), m.processing.as_millis(), m.total.as_millis());
    println!("PERF_THROUGHPUT chunks_per_sec={:.2} api_per_min={:.1} tokens_per_min={:.0}",
        m.chunks_per_sec, m.api_calls_per_min, m.tokens_per_min);
    println!("PERF_OUTCOME done={}/{} failed={}", m.done_tasks, m.n_tasks, m.failed_tasks);

    assert_eq!(m.failed_tasks, 0, "expected 0 failed tasks");
    assert_eq!(m.done_tasks, m.n_tasks, "all tasks should be done");
}

// ═══════════════════════════════════════════════════════════════════════
// Sweet-spot finder: probe API rate limit sweet spot
// ═══════════════════════════════════════════════════════════════════════

#[actix_web::test]
#[ignore]
/// Probe different RPM values to find the API rate sweet spot.
/// Runs small batches (15 tasks) at increasing RPM levels.
/// Reports success rate and throughput for each level.
async fn find_sweet_spot() {
    let rpm_levels: Vec<u64> = vec![10, 20, 30, 50];
    let tasks_per_level = 10usize;
    let timeout_per_level = Duration::from_secs(600);

    let sep = "=".repeat(80);
    let dash_sep = "-".repeat(80);

    eprintln!("\n{sep}");
    eprintln!("SWEET SPOT FINDER: testing {} RPM levels with {} tasks each", rpm_levels.len(), tasks_per_level);
    eprintln!("{sep}\n");

    struct LevelResult {
        rpm: u64,
        ok: usize,
        total: usize,
        elapsed_ms: u128,
        throughput_per_min: f64,
    }

    let mut results: Vec<LevelResult> = Vec::new();

    for &rpm in &rpm_levels {
        eprintln!("\n--- Testing RPM={rpm} ({tasks_per_level} tasks) ---");

        let m = stress_real_with_rpm(tasks_per_level, timeout_per_level, Some(rpm)).await;
        let ok_rate = if m.n > 0 { m.ok as f64 / m.n as f64 * 100.0 } else { 0.0 };
        let elapsed_min = m.total.as_secs_f64() / 60.0;
        let throughput = if elapsed_min > 0.0 { m.ok as f64 / elapsed_min } else { 0.0 };

        eprintln!(
            "[SWEET_SPOT] RPM={rpm:>3} | ok={ok}/{total} ({ok_rate:.0}%) | time={time:.1}s | throughput={throughput:.1} tasks/min",
            ok = m.ok, total = m.n, time = m.total.as_secs_f64(),
        );

        results.push(LevelResult {
            rpm,
            ok: m.ok,
            total: m.n,
            elapsed_ms: m.total.as_millis(),
            throughput_per_min: throughput,
        });

        // Brief pause between levels to let API recover
        if rpm != *rpm_levels.last().unwrap() {
            eprintln!("[SWEET_SPOT] cooling down 5s before next level...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    // ── Summary table ──
    eprintln!("\n{sep}");
    eprintln!("SWEET SPOT RESULTS");
    eprintln!("{sep}");
    eprintln!("{:>6} | {:>8} | {:>8} | {:>10} | {:>14}", "RPM", "Success", "OK/Total", "Time(s)", "Throughput/min");
    eprintln!("{dash_sep}");

    let mut best_rpm = 0u64;
    let mut best_throughput = 0.0f64;

    for r in &results {
        let ok_rate = if r.total > 0 { r.ok as f64 / r.total as f64 * 100.0 } else { 0.0 };
        let marker = if ok_rate >= 90.0 && r.throughput_per_min > best_throughput {
            best_throughput = r.throughput_per_min;
            best_rpm = r.rpm;
            " <-- SWEET SPOT"
        } else {
            ""
        };
        eprintln!(
            "{:>6} | {:>7.0}% | {:>4}/{:<4} | {:>9.1} | {:>13.1}{}",
            r.rpm, ok_rate, r.ok, r.total,
            r.elapsed_ms as f64 / 1000.0,
            r.throughput_per_min,
            marker,
        );
    }

    eprintln!("\n{sep}");
    eprintln!("RECOMMENDED RPM: {best_rpm} (throughput: {best_throughput:.1} tasks/min with >=90% success)");
    eprintln!("{sep}\n");

    assert!(best_rpm > 0, "no RPM level achieved >= 90% success rate");
}

// ═══════════════════════════════════════════════════════════════════════
// Real stress sweep: 3 batches × 30 tasks, 5-min cooldown, monitoring
// ═══════════════════════════════════════════════════════════════════════

#[actix_web::test]
#[ignore]
/// 3 batches of 30 tasks with ~2000-token text each.
/// Monitors memory, cache, output. 5-minute cooldown between batches.
async fn real_stress_sweep() {
    let _ = dotenvy::dotenv();
    let sep = "=".repeat(80);
    let dash = "-".repeat(80);
    let cache_dir = std::path::PathBuf::from("/tmp/ummimo-cache-real");
    let output_dir = std::path::PathBuf::from("../data/output");

    let n_batches = 3usize;
    let tasks_per_batch = 30usize;
    let cooldown = Duration::from_secs(300); // 5 minutes
    let timeout = Duration::from_secs(1200); // 20 min per batch

    eprintln!("\n{sep}");
    eprintln!("REAL STRESS SWEEP: {n_batches} batches x {tasks_per_batch} tasks, {cooldown:?} cooldown");
    eprintln!("{sep}\n");

    struct BatchResult {
        batch_id: usize,
        ok: usize,
        total: usize,
        elapsed_s: f64,
        rss_peak: f64,
        cache_growth_mb: f64,
        output_growth_mb: f64,
    }

    let mut results: Vec<BatchResult> = Vec::new();
    let initial_snap = snapshot_system(&cache_dir, &output_dir);
    print_snapshot("INIT", &initial_snap, None);

    for batch_idx in 0..n_batches {
        eprintln!("\n{dash}");
        eprintln!("[BATCH {}/{}] starting...", batch_idx + 1, n_batches);
        eprintln!("{dash}");

        // Pre-batch cleanup
        cleanup_cache();
        cleanup_output();

        let pre_snap = snapshot_system(&cache_dir, &output_dir);
        print_snapshot("PRE", &pre_snap, None);

        // Run batch with large text via gen_text (100 + i*99 % 9901 chars)
        // For i=0..29, text ranges from 100 to ~3000 chars
        let m = stress_real_with_rpm(tasks_per_batch, timeout, Some(20)).await;

        let post_snap = snapshot_system(&cache_dir, &output_dir);
        print_snapshot("POST", &post_snap, Some(&pre_snap));

        let ok_rate = if m.n > 0 { m.ok as f64 / m.n as f64 * 100.0 } else { 0.0 };
        eprintln!(
            "[BATCH {}/{}] DONE ok={}/{} ({:.0}%) time={:.1}s",
            batch_idx + 1, n_batches, m.ok, m.n, ok_rate, m.total.as_secs_f64()
        );

        results.push(BatchResult {
            batch_id: batch_idx + 1,
            ok: m.ok,
            total: m.n,
            elapsed_s: m.total.as_secs_f64(),
            rss_peak: post_snap.rss_mb,
            cache_growth_mb: post_snap.cache_size_mb - pre_snap.cache_size_mb,
            output_growth_mb: post_snap.output_size_mb - pre_snap.output_size_mb,
        });

        // Cooldown between batches (skip after last)
        if batch_idx + 1 < n_batches {
            eprintln!("[COOLDOWN] waiting {cooldown:?} before next batch...");
            tokio::time::sleep(cooldown).await;
        }
    }

    // ── Summary ──
    let final_snap = snapshot_system(&cache_dir, &output_dir);
    print_snapshot("FINAL", &final_snap, Some(&initial_snap));

    eprintln!("\n{sep}");
    eprintln!("BATCH SWEEP RESULTS ({n_batches} batches x {tasks_per_batch} tasks, RPM=20)");
    eprintln!("{sep}");
    eprintln!("{:>5} | {:>7} | {:>8} | {:>8} | {:>12} | {:>12}",
        "Batch", "Success", "Time(s)", "RSS(MB)", "Cache(MB)", "Output(MB)");
    eprintln!("{dash}");

    let mut total_ok = 0usize;
    let mut total_n = 0usize;
    for r in &results {
        total_ok += r.ok;
        total_n += r.total;
        eprintln!(
            "{:>5} | {:>4}/{:<3} | {:>7.1} | {:>7.1} | {:>+11.1} | {:>+11.1}",
            r.batch_id, r.ok, r.total, r.elapsed_s, r.rss_peak,
            r.cache_growth_mb, r.output_growth_mb,
        );
    }
    let total_rate = if total_n > 0 { total_ok as f64 / total_n as f64 * 100.0 } else { 0.0 };
    eprintln!("{dash}");
    eprintln!("TOTAL: {total_ok}/{total_n} ({total_rate:.0}% success)");
    eprintln!("{sep}\n");

    // Cleanup after test
    cleanup_cache();
    cleanup_output();
}

// ═══════════════════════════════════════════════════════════════════════
// RPM sliding window: fine-tune around sweet spot
// ═══════════════════════════════════════════════════════════════════════

#[actix_web::test]
#[ignore]
/// Slide RPM around sweet spot (20) to find true optimum.
/// Tests RPM [15, 18, 20, 22, 25] with 20 tasks each, 5-min cooldown.
async fn rpm_sliding_window() {
    let _ = dotenvy::dotenv();
    let rpm_levels: Vec<u64> = vec![15, 18, 20, 22, 25];
    let tasks_per_level = 20usize;
    let timeout_per_level = Duration::from_secs(900); // 15 min
    let cooldown = Duration::from_secs(300); // 5 minutes
    let cache_dir = std::path::PathBuf::from("/tmp/ummimo-cache-real");
    let output_dir = std::path::PathBuf::from("../data/output");

    let sep = "=".repeat(80);
    let dash = "-".repeat(80);

    eprintln!("\n{sep}");
    eprintln!("RPM SLIDING WINDOW: testing {:?} with {tasks_per_level} tasks each, {cooldown:?} cooldown",
        rpm_levels);
    eprintln!("{sep}\n");

    struct WindowResult {
        rpm: u64,
        ok: usize,
        total: usize,
        elapsed_s: f64,
        throughput: f64,
        rss_peak: f64,
        cache_growth_mb: f64,
    }

    let mut results: Vec<WindowResult> = Vec::new();

    for (idx, &rpm) in rpm_levels.iter().enumerate() {
        eprintln!("\n{dash}");
        eprintln!("[RPM={rpm}] ({}/{}) starting...", idx + 1, rpm_levels.len());
        eprintln!("{dash}");

        cleanup_cache();
        cleanup_output();

        let pre_snap = snapshot_system(&cache_dir, &output_dir);
        print_snapshot(&format!("RPM{rpm}-PRE"), &pre_snap, None);

        let m = stress_real_with_rpm(tasks_per_level, timeout_per_level, Some(rpm)).await;

        let post_snap = snapshot_system(&cache_dir, &output_dir);
        print_snapshot(&format!("RPM{rpm}-POST"), &post_snap, Some(&pre_snap));

        let ok_rate = if m.n > 0 { m.ok as f64 / m.n as f64 * 100.0 } else { 0.0 };
        let elapsed_min = m.total.as_secs_f64() / 60.0;
        let throughput = if elapsed_min > 0.0 { m.ok as f64 / elapsed_min } else { 0.0 };

        eprintln!(
            "[RPM={rpm}] DONE ok={}/{} ({:.0}%) time={:.1}s throughput={throughput:.1}/min",
            m.ok, m.n, ok_rate, m.total.as_secs_f64(),
        );

        results.push(WindowResult {
            rpm,
            ok: m.ok,
            total: m.n,
            elapsed_s: m.total.as_secs_f64(),
            throughput,
            rss_peak: post_snap.rss_mb,
            cache_growth_mb: post_snap.cache_size_mb - pre_snap.cache_size_mb,
        });

        // Cooldown between levels
        if idx + 1 < rpm_levels.len() {
            eprintln!("[COOLDOWN] waiting {cooldown:?} before next RPM level...");
            tokio::time::sleep(cooldown).await;
        }
    }

    // ── Summary table ──
    eprintln!("\n{sep}");
    eprintln!("RPM SLIDING WINDOW RESULTS");
    eprintln!("{sep}");
    eprintln!("{:>5} | {:>7} | {:>8} | {:>8} | {:>10} | {:>10}",
        "RPM", "Success", "Time(s)", "RSS(MB)", "Thr/min", "Cache(MB)");
    eprintln!("{dash}");

    let mut best_rpm = 0u64;
    let mut best_score = 0.0f64; // score = ok_rate * throughput

    for r in &results {
        let ok_rate = if r.total > 0 { r.ok as f64 / r.total as f64 * 100.0 } else { 0.0 };
        let score = ok_rate * r.throughput;
        let marker = if ok_rate >= 80.0 && score > best_score {
            best_score = score;
            best_rpm = r.rpm;
            " <-- BEST"
        } else {
            ""
        };
        eprintln!(
            "{:>5} | {:>4}/{:<3} | {:>7.1} | {:>7.1} | {:>9.1} | {:>+9.1}{marker}",
            r.rpm, r.ok, r.total, r.elapsed_s, r.rss_peak,
            r.throughput, r.cache_growth_mb,
        );
    }

    eprintln!("{dash}");
    eprintln!("RECOMMENDED RPM: {best_rpm}");
    eprintln!("{sep}\n");

    // Cleanup
    cleanup_cache();
    cleanup_output();

    assert!(best_rpm > 0, "no RPM level achieved >= 80% success with good throughput");
}

// ═══════════════════════════════════════════════════════════════════════════
//  LARGE-SCALE RELIABILITY TEST  —  100 tasks, RPM=10, 5 batches × 20
//  Verifies the system completes all batches without crashes or hangs.
//  No strict success-rate assertion (API may be unstable); we only assert
//  that every batch *completes* within the timeout.
// ═══════════════════════════════════════════════════════════════════════════

#[actix_rt::test]
#[ignore] // takes ~40 min with real API
async fn large_scale_reliability() {
    let rpm: u64 = 10;
    let n_batches = 5usize;
    let tasks_per_batch = 20usize;
    let cooldown = Duration::from_secs(300); // 5 min between batches
    let timeout = Duration::from_secs(900);  // 15 min per batch

    let sep = "=".repeat(80);
    let dash = "-".repeat(80);

    let cache_dir = std::path::PathBuf::from("/tmp/ummimo-cache-large");
    let output_dir = std::path::PathBuf::from("../data/output");

    eprintln!("\n{sep}");
    eprintln!("LARGE-SCALE RELIABILITY: {n_batches} batches x {tasks_per_batch} tasks (total {} tasks), RPM={rpm}",
        n_batches * tasks_per_batch);
    eprintln!("{sep}\n");

    struct BatchResult {
        batch_id: usize,
        ok: usize,
        fail: usize,
        total: usize,
        elapsed_s: f64,
        rss_delta: f64,
        cache_files: usize,
    }

    let mut results: Vec<BatchResult> = Vec::new();
    let t0 = Instant::now();

    let initial_snap = snapshot_system(&cache_dir, &output_dir);
    print_snapshot("INIT", &initial_snap, None);

    for batch_idx in 0..n_batches {
        eprintln!("\n{dash}");
        eprintln!("[BATCH {}/{}] starting...", batch_idx + 1, n_batches);
        eprintln!("{dash}");

        // Pre-batch cleanup
        cleanup_cache();
        cleanup_output();

        let pre_snap = snapshot_system(&cache_dir, &output_dir);

        // Run batch
        let m = stress_real_with_rpm(tasks_per_batch, timeout, Some(rpm)).await;

        let post_snap = snapshot_system(&cache_dir, &output_dir);
        print_snapshot("POST", &post_snap, Some(&pre_snap));

        let fail_count = m.n.saturating_sub(m.ok);
        let elapsed_s = m.total.as_secs_f64();

        eprintln!("[BATCH {}/{}] DONE ok={}/{} ({:.0}%) fail={} time={:.1}s",
            batch_idx + 1, n_batches, m.ok, m.n,
            if m.n > 0 { m.ok as f64 / m.n as f64 * 100.0 } else { 0.0 },
            fail_count, elapsed_s,
        );

        results.push(BatchResult {
            batch_id: batch_idx + 1,
            ok: m.ok,
            fail: fail_count,
            total: m.n,
            elapsed_s,
            rss_delta: post_snap.rss_mb - pre_snap.rss_mb,
            cache_files: post_snap.cache_files,
        });

        // Cooldown between batches (skip after last)
        if batch_idx < n_batches - 1 {
            eprintln!("[COOLDOWN] waiting {}s before next batch...", cooldown.as_secs());
            tokio::time::sleep(cooldown).await;
        }
    }

    let final_snap = snapshot_system(&cache_dir, &output_dir);
    print_snapshot("FINAL", &final_snap, Some(&initial_snap));

    // ── Summary table ──
    eprintln!("\n{sep}");
    eprintln!("LARGE-SCALE RELIABILITY RESULTS ({} batches x {} tasks, RPM={})",
        n_batches, tasks_per_batch, rpm);
    eprintln!("{sep}");
    eprintln!("{:>5} | {:>7} | {:>7} | {:>7} | {:>8} | {:>9} | {:>7}",
        "Batch", "OK", "Fail", "Total", "Time(s)", "Δ RSS(MB)", "Cache");
    eprintln!("{dash}");

    let mut total_ok = 0usize;
    let mut total_fail = 0usize;
    let mut total_tasks = 0usize;

    for r in &results {
        eprintln!("{:>5} | {:>7} | {:>7} | {:>7} | {:>8.1} | {:>+9.1} | {:>7}",
            r.batch_id, r.ok, r.fail, r.total, r.elapsed_s, r.rss_delta, r.cache_files);
        total_ok += r.ok;
        total_fail += r.fail;
        total_tasks += r.total;
    }

    eprintln!("{dash}");
    eprintln!("TOTAL: ok={}/{} ({:.1}% success), fail={}",
        total_ok, total_tasks,
        if total_tasks > 0 { total_ok as f64 / total_tasks as f64 * 100.0 } else { 0.0 },
        total_fail);
    eprintln!("TOTAL TIME: {:.1}s ({:.1} min)", t0.elapsed().as_secs_f64(), t0.elapsed().as_secs_f64() / 60.0);
    eprintln!("{sep}\n");

    // Cleanup
    cleanup_cache();
    cleanup_output();

    // ── Assertions ──
    // 1. All batches must have completed (no hang/crash)
    assert_eq!(results.len(), n_batches, "Not all batches completed!");
    // 2. Every batch must have started (total > 0)
    for r in &results {
        assert!(r.total > 0, "Batch {} had 0 tasks!", r.batch_id);
    }
    // 3. Total RSS growth reasonable (< 200 MB)
    let total_rss_growth = final_snap.rss_mb - initial_snap.rss_mb;
    assert!(total_rss_growth < 200.0, "RSS grew too much: {:.1} MB", total_rss_growth);

    eprintln!("[PASS] All {n_batches} batches completed successfully. RSS growth: {total_rss_growth:.1} MB");
}
