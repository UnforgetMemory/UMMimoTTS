//! v4 assembly / cleanup / resume integration tests (wiremock upstream).
//!
//! Covers the perf-correctness fixes:
//! 1. multi-chunk tasks merge in seq order and RECLAIM chunk PCM files;
//! 2. re-chunk (context overflow) keeps exactly ONE chunk generation —
//!    stale done chunks can never leak duplicate audio into the merge;
//! 3. a restarted engine resumes instantly (startup stale-inflight reset).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use mimotts_core::chunking::ChunkConfig;
use mimotts_core::domain::CreateTaskInput;
use mimotts_engine::{Engine, EngineConfig};

/// 0.05s of 24kHz mono pcm16 (2400 bytes).
const PCM: [u8; 2400] = [0u8; 2400];

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mimotts-{tag}-{}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cfg_in(dir: &Path, workers: usize, chunk: ChunkConfig) -> EngineConfig {
    EngineConfig {
        db_path: dir.join("mimo.db").to_string_lossy().to_string(),
        data_dir: dir.to_path_buf(),
        output_dir: dir.join("output"),
        workers,
        rpm_headroom: 1_000_000, // integration: no throttle ceiling
        tpm_budget: 10_000_000_000,
        chunk,
        max_window: 8,
        stream_audio: true,
        announcement: None,
    }
}

async fn mount_streaming_ok(server: &MockServer, delay_ms: u64) {
    let b64 = B64.encode(PCM);
    let body = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"audio\":{{\"data\":\"{b64}\"}}}}}}]}}\n\n\
         data: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(body, "text/event-stream")
                .set_delay(Duration::from_millis(delay_ms)),
        )
        .mount(server)
        .await;
}

async fn engine_on(dir: &Path, server: &MockServer, workers: usize, chunk: ChunkConfig) -> Arc<Engine> {
    let engine = Engine::open(cfg_in(dir, workers, chunk)).unwrap();
    engine.set_provider_key("xiaomi", "test-key").unwrap();
    let conn = engine.storage.pool.get().unwrap();
    conn.execute(
        "UPDATE providers SET base_url=?1 WHERE id='xiaomi'",
        rusqlite::params![server.uri()],
    )
    .unwrap();
    engine
}

async fn wait_terminal(engine: &Engine, task_id: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some((task, _chunks)) = engine.task(task_id).unwrap() {
            if task.status.is_terminal() {
                return;
            }
        }
        assert!(Instant::now() < deadline, "task {task_id} never reached terminal");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn multi_chunk_task_merges_and_reclaims_chunk_files() {
    let server = MockServer::start().await;
    mount_streaming_ok(&server, 10).await;
    let dir = scratch_dir("assembly");
    let engine = engine_on(
        &dir,
        &server,
        4,
        ChunkConfig { target_tokens: 4, hard_cap_tokens: 8000 },
    )
    .await;

    // Each sentence (~6 est. tokens) exceeds the tiny target → 3 chunks.
    let task = engine
        .submit_task(CreateTaskInput {
            session_id: None,
            title: "order-test".into(),
            content: "第一句。第二句。第三句。".into(),
            voice: "mimo_default".into(),
            model: "mimo-v2.5-tts".into(),
            style: None,
            priority: 0,
            provider_id: None,
        })
        .unwrap();
    wait_terminal(&engine, &task.id.to_string(), Duration::from_secs(20)).await;

    let (_t, chunks) = engine.task(&task.id.to_string()).unwrap().unwrap();
    assert_eq!(chunks.len(), 3, "expected 3 chunks, got {:?}", chunks.len());
    assert!(chunks.iter().all(|c| c.status == "done"), "all chunks done");

    // Merged WAV: 44-byte header + 3 × 2400 bytes of pcm16, byte-exact.
    let out = dir.join("output").join(format!("{}.wav", task.id));
    let wav = std::fs::read(&out).expect("merged wav exists");
    assert_eq!(wav.len(), 44 + 3 * PCM.len());
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    // Verify exact sizes were patched into the header.
    let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
    assert_eq!(data_len, 3 * PCM.len());

    // Storage hygiene: chunk PCM files and the live raw stream are reclaimed.
    let chunks_dir = dir.join("chunks");
    assert_eq!(
        std::fs::read_dir(&chunks_dir).map(|d| d.count()).unwrap_or(0),
        0,
        "chunk pcm files must be reclaimed after merge"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.join("output"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".pcm.tmp"))
        .collect();
    assert!(leftovers.is_empty(), "live raw stream must be removed: {leftovers:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn rechunk_keeps_single_generation() {
    let server = MockServer::start().await;
    mount_streaming_ok(&server, 10).await;
    // First request only → 400 context overflow (mount AFTER success so
    // wiremock checks it first, reverse mount order).
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string("context length exceeded"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let dir = scratch_dir("rechunk");
    let engine = engine_on(&dir, &server, 1, ChunkConfig::default()).await;
    let task = engine
        .submit_task(CreateTaskInput {
            session_id: None,
            title: "rechunk-test".into(),
            content: "这是一段会触发上下文超限的文本。".into(),
            voice: "mimo_default".into(),
            model: "mimo-v2.5-tts".into(),
            style: None,
            priority: 0,
            provider_id: None,
        })
        .unwrap();
    wait_terminal(&engine, &task.id.to_string(), Duration::from_secs(20)).await;

    // Single generation: the stale chunk row was wiped by re-chunk, so the
    // task ends with exactly the final chunk set — no duplicates to merge.
    let (t, chunks) = engine.task(&task.id.to_string()).unwrap().unwrap();
    assert_eq!(t.status, mimotts_core::domain::TaskStatus::Done);
    assert_eq!(chunks.len(), 1, "exactly one generation survives");
    assert!(chunks.iter().all(|c| c.status == "done"));
    assert_eq!(t.total_chunks, 1);

    let out = dir.join("output").join(format!("{}.wav", task.id));
    let wav = std::fs::read(&out).expect("merged wav exists");
    assert_eq!(wav.len(), 44 + PCM.len(), "audio contains exactly one chunk");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn restart_resumes_pending_and_orphaned_inflight() {
    let server = MockServer::start().await;
    mount_streaming_ok(&server, 300).await; // slow enough to interrupt mid-run
    let dir = scratch_dir("resume");

    // ── engine A: import 12 tasks, kill it mid-flight ──
    {
        let engine = engine_on(&dir, &server, 2, ChunkConfig::default()).await;
        let files: Vec<(String, Vec<u8>)> = (0..12)
            .map(|i| (format!("r{i:02}.txt"), format!("第{i}章，测试文本。").into_bytes()))
            .collect();
        engine
            .import_files(
                None,
                Some("resume-test".into()),
                "mimo_default",
                "mimo-v2.5-tts",
                None,
                None,
                files,
            )
            .unwrap();
        tokio::time::sleep(Duration::from_millis(900)).await; // ~3 chunks settle
        engine.shutdown();
        drop(engine);
        // Let detached workers finish their in-flight syntheses and exit.
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    // ── engine B: same db → pending chunks re-seeded, stale inflight reset ──
    {
        let engine = engine_on(&dir, &server, 2, ChunkConfig::default()).await;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            // Find the resumed session by scanning sessions.
            let (_rows, total) = engine.list_sessions(0, 10).unwrap();
            assert_eq!(total, 1, "one session survives the restart");
            let row = &engine.list_sessions(0, 10).unwrap().0[0];
            if matches!(row.status.as_str(), "completed" | "failed") {
                break;
            }
            assert!(Instant::now() < deadline, "resume timed out");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let rows = engine.list_sessions(0, 10).unwrap().0;
        assert_eq!(rows[0].done_tasks, 12, "all 12 tasks complete after restart");
        assert_eq!(rows[0].failed_tasks, 0);
        // Every output exists and is complete.
        let mut wavs = 0;
        if let Ok(rd) = std::fs::read_dir(dir.join("output")) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "wav").unwrap_or(false) {
                    wavs += 1;
                }
            }
        }
        assert_eq!(wavs, 12, "every task produced a merged wav");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
