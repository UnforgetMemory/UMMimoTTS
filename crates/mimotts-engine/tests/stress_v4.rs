//! W3 stress gate: 1000-file TXT import through the whole v4 pipeline
//! with a wiremock upstream that mimics the official streaming pcm16 API.
//!
//! Run: `cargo test -p mimotts-engine --test stress_v4 -- --nocapture`

use std::time::Instant;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use mimotts_engine::{Engine, EngineConfig};
use mimotts_core::chunking::ChunkConfig;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thousand_file_import_completes() {
    // ── upstream mock: official streaming contract (pcm16 deltas + [DONE]) ──
    let server = MockServer::start().await;
    let pcm = vec![0u8; 24_000]; // 0.5s of 24kHz mono pcm16
    let b64 = B64.encode(&pcm);
    let body = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"audio\":{{\"data\":\"{b64}\"}}}}}}]}}\n\n\
         data: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(body, "text/event-stream")
                // ~15ms synthesis latency per chunk
                .set_delay(std::time::Duration::from_millis(15)),
        )
        .expect(1000)
        .mount(&server)
        .await;

    // ── engine in a scratch dir ──
    let dir = std::env::temp_dir().join(format!("mimotts-stress-{}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = EngineConfig {
        db_path: dir.join("mimo.db").to_string_lossy().to_string(),
        data_dir: dir.clone(),
        output_dir: dir.join("output"),
        workers: 4,
        rpm_headroom: 1000, // stress: no throttle ceiling
        tpm_budget: 10_000_000,
        chunk: ChunkConfig::default(),
        max_window: 16,
        stream_audio: true,
        announcement: None,
    };
    let engine = Engine::open(cfg).unwrap();
    engine
        .set_provider_key("xiaomi", "test-key")
        .unwrap();
    // point the seeded provider at the mock
    {
        let conn = engine.storage.pool.get().unwrap();
        conn.execute(
            "UPDATE providers SET base_url=?1 WHERE id='xiaomi'",
            rusqlite::params![server.uri()],
        )
        .unwrap();
    }

    // ── 1000 txt files ──
    let files: Vec<(String, Vec<u8>)> = (0..1000)
        .map(|i| {
            (
                format!("chapter_{i:04}.txt"),
                format!("第{i}章，这是压力测试文本。共两句，第二句更短。").into_bytes(),
            )
        })
        .collect();

    let t0 = Instant::now();
    let result = engine
        .import_files(
            None,
            Some("压测会话".into()),
            "冰糖",
            "mimo-v2.5-tts",
            Some("沉稳".into()),
            None,
            files,
        )
        .unwrap();
    assert_eq!(result.tasks_created, 1000);
    assert!(result.rejected.is_empty(), "rejected: {:?}", result.rejected);

    // ── wait for terminal ──
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let row = engine.session(&result.session_id).unwrap().unwrap();
        if matches!(row.status.as_str(), "completed" | "failed") {
            let elapsed = t0.elapsed();
            println!(
                "stress: status={} done={} failed={} elapsed={:?} queue={}",
                row.status,
                row.done_tasks,
                row.failed_tasks,
                elapsed,
                engine.stats()["queue_depth"]
            );
            if row.failed_tasks > 0 {
                // diagnostics: print failed chunk errors
                let conn = engine.storage.pool.get().unwrap();
                let mut stmt = conn
                    .prepare(
                        "SELECT c.task_id, c.error FROM chunks c
                         JOIN tasks t ON t.id = c.task_id
                         WHERE t.session_id = ?1 AND c.status = 'failed' LIMIT 5",
                    )
                    .unwrap();
                let rows: Vec<(String, Option<String>)> = stmt
                    .query_map(rusqlite::params![result.session_id], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                for (tid, err) in &rows {
                    println!("  failed chunk: task={tid} error={:?}", err);
                }
            }
            assert_eq!(row.done_tasks, 1000, "all 1000 tasks must complete");
            assert_eq!(row.failed_tasks, 0);
            return;
        }
        if std::time::Instant::now() > deadline {
            let row = engine.session(&result.session_id).unwrap().unwrap();
            panic!(
                "timeout: status={} done={} failed={} total={}",
                row.status, row.done_tasks, row.failed_tasks, row.total_tasks
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[test]
fn chunker_handles_typical_txt_files() {
    // Sanity: a 10KB mixed Chinese/English file splits losslessly.
    let mut text = String::new();
    for i in 0..300 {
        text.push_str(&format!("第{i}句中文内容，加上一些 English words mixed in. "));
    }
    let chunks = mimotts_core::chunking::split(&text, None, &ChunkConfig::default());
    assert!(chunks.len() >= 2, "expected multiple chunks");
    let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(joined, mimotts_core::chunking::normalize_whitespace(&text));
}
