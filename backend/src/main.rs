//! UMMimoTTS v3 — Binary entry point.
//!
//! Wires together all infrastructure, services, and routes,
//! then starts the actix-web HTTP server.
//!
//! Environment variables:
//!   MIMO_API_KEY         — MIMO TTS API key (default: "test-key")
//!   MIMO_BASE_URL        — MIMO API base URL  (default: "http://localhost:30231")
//!   SERVER_PORT          — HTTP listen port    (default: 30231)
//!   DB_PATH              — SQLite file path    (default: "data/mimo.db")
//!   MAX_CONCURRENT       — ChunkQueue concurrency (default: 2)
//!   CACHE_DIR            — Audio cache directory   (default: "data/cache")

use actix_cors::Cors;
use actix_web::{web, App, HttpServer, middleware};
use std::sync::Arc;
use std::time::Duration;

use um_mimo_tts_server::domain::events::DomainEvent;
use um_mimo_tts_server::infra::persistence::db::create_pool;
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
use um_mimo_tts_server::service::task_service::TaskService;
use um_mimo_tts_server::service::batch_service::BatchService;
use um_mimo_tts_server::service::group_service::GroupService;
use um_mimo_tts_server::routes::AppState;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // ── load .env if present ──────────────────────────────────────────
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let port: u16 = env_or("SERVER_PORT", "30231").parse().expect("SERVER_PORT must be a u16");
    let db_path = env_or("DB_PATH", "data/mimo.db");
    let mimo_api_key = env_or("MIMO_API_KEY", "test-key");
    let mimo_base_url = env_or("MIMO_BASE_URL", "http://localhost:30231");
    let max_concurrent: usize = env_or("MAX_CONCURRENT", "2").parse().expect("MAX_CONCURRENT must be usize");
    let cache_dir = std::path::PathBuf::from(env_or("CACHE_DIR", "data/cache"));
    let _max_task_wait = Duration::from_secs(300);

    // ── database ──────────────────────────────────────────────────────
    let pool = create_pool(&db_path, 10).expect("Failed to create DB pool");
    {
        let conn = pool.get().expect("Failed to get DB connection");
        run_migrations(&conn).expect("Failed to run migrations");
    }
    tracing::info!("Database ready at {db_path}");

    // ── repos ─────────────────────────────────────────────────────────
    let task_repo: Arc<dyn TaskRepo> = Arc::new(SqliteTaskRepo::new(pool.clone()));
    let chunk_repo: Arc<dyn ChunkRepo> = Arc::new(SqliteChunkRepo::new(pool.clone()));
    let batch_repo: Arc<dyn BatchRepo> = Arc::new(SqliteBatchRepo::new(pool.clone()));
    let group_repo: Arc<dyn GroupRepo> = Arc::new(SqliteGroupRepo::new(pool.clone()));

    // ── event bus ─────────────────────────────────────────────────────
    let (event_tx, event_rx) = tokio::sync::broadcast::channel::<DomainEvent>(256);
    // Second receiver for TaskQueue event listener (broadcast receivers are independent)
    let task_event_rx = event_tx.subscribe();

    // ── SSE bus ───────────────────────────────────────────────────────
    let sse_bus = Arc::new(SseBus::new());

    // ── SSE bridge: forward domain events to SSE subscribers ─────────
    um_mimo_tts_server::infra::sse_bus::spawn_sse_bridge(event_rx, sse_bus.clone());

    // ── MIMO client + chunker ─────────────────────────────────────────
    let client = Arc::new(MimoClient::new(&mimo_api_key, &mimo_base_url));
    let chunker = MimoChunker::new(&mimo_base_url, 2000, 5000);

    // ── cache ─────────────────────────────────────────────────────────
    let cache = Arc::new(Cache::new(cache_dir.clone(), Duration::from_secs(3600), 100));

    // ── rate limiter ──────────────────────────────────────────────────
    let rate_limiter = Arc::new(TokenBucket::new(100));
    let token_budget = Arc::new(TokenBucket::new(1_000_000));

    // ── queues ────────────────────────────────────────────────────────
    let chunk_queue = Arc::new(ChunkQueue::new(
        pool.clone(),
        chunk_repo.clone(),
        task_repo.clone(),
        client.clone(),
        cache.clone(),
        rate_limiter.clone(),
        token_budget.clone(),
        event_tx.clone(),
        max_concurrent,
        Duration::from_secs(30),
        cache_dir.clone(),
    ));

    let task_queue = Arc::new(TaskQueue::new(
        pool.clone(),
        task_repo.clone(),
        chunk_repo.clone(),
        chunk_queue.clone(),
        event_tx.clone(),
        chunker,
    ));

    // ── services ──────────────────────────────────────────────────────
    let task_service = Arc::new(TaskService::new(task_repo.clone(), chunk_repo.clone(), task_queue.clone()));
    let group_service = Arc::new(GroupService::new(group_repo.clone()));
    let batch_service = Arc::new(BatchService::new(
        batch_repo.clone(),
        task_service.clone(),
        sse_bus.clone(),
    ));

    let app_state = AppState {
        batch_service,
        task_service,
        group_service,
        sse_bus,
    };

    // ── start queue workers ──────────────────────────────────────────
    chunk_queue.run_workers();
    tracing::info!("ChunkQueue workers started (max_concurrent={max_concurrent})");

    // ── start task queue event listener ─────────────────────────────
    let tq = task_queue.clone();
    tokio::spawn(async move {
        tq.listen(task_event_rx).await;
    });
    tracing::info!("TaskQueue event listener started");

    // ── HTTP server ───────────────────────────────────────────────────
    tracing::info!("Starting server on 0.0.0.0:{port}");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit(50 * 1024 * 1024)) // 50 MB for batch add
            .app_data(web::Data::new(app_state.clone()))
            .configure(um_mimo_tts_server::routes::configure)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
