//! `mimotts serve` — HTTP server assembly.

use std::path::PathBuf;

use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};

use mimotts_engine::{Engine, EngineConfig};

pub struct ServeOptions {
    pub data_dir: PathBuf,
    pub db: PathBuf,
    pub port: u16,
    pub bind: String,
    pub headless: bool,
    pub ui_dist: Option<PathBuf>,
    pub workers: usize,
    pub stream_audio: bool,
}

pub async fn run(opts: ServeOptions) -> Result<(), mimotts_engine::EngineError> {
    let _ = dotenvy::dotenv();
    let mut cfg = EngineConfig {
        db_path: opts.db.to_string_lossy().to_string(),
        data_dir: opts.data_dir.clone(),
        workers: opts.workers,
        stream_audio: opts.stream_audio,
        ..Default::default()
    };
    mimotts_engine::apply_env_overrides(&mut cfg);
    let engine = Engine::open(cfg)?;

    // E2E/local override: MIMOTTS_BASE_URL repoints seeded providers (mock upstream).
    if let Ok(upstream) = std::env::var("MIMOTTS_BASE_URL") {
        engine.override_provider_base_urls(upstream.trim())?;
        tracing::info!("provider base URLs overridden to {upstream}");
    }

    // First-run token bootstrap (ADR-007).
    if !engine.has_any_token()? {
        let token = engine.issue_token("bootstrap")?;
        println!();
        println!("════════════════════════════════════════════════════════");
        println!("  首次运行：已生成 API Token（仅显示一次，请复制保存）");
        println!("════════════════════════════════════════════════════════");
        println!("  {token}");
        println!("════════════════════════════════════════════════════════");
        println!();
    }

    let ui_dist = opts.ui_dist.or_else(|| {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/web/dist")
            .canonicalize()
            .ok();
        p
    });
    if !opts.headless {
        match &ui_dist {
            Some(dir) if dir.exists() => tracing::info!("serving UI from {}", dir.display()),
            _ => tracing::warn!(
                "UI dist not found — API only. Build apps/web first (npm run build)."
            ),
        }
    } else {
        tracing::info!("headless mode: API only");
    }

    let app_state = mimotts_api::AppState::new(engine.clone(), ui_dist.clone());
    let bind = opts.bind.clone();
    let port = opts.port;

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:30232")
            .allowed_origin("http://127.0.0.1:30232")
            .allowed_origin("http://localhost:5173")
            .allowed_origin("http://127.0.0.1:5173")
            .allowed_origin(&format!("http://localhost:{port}"))
            .allowed_origin(&format!("http://127.0.0.1:{port}"))
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        let app = App::new()
            .wrap(cors)
            // No `%r`: query strings carry `?token=` and must never hit logs.
            .wrap(middleware::Logger::new("%a \"%m %U\" %s %b %T"))
            .app_data(web::JsonConfig::default().limit(4 * 1024 * 1024))
            .app_data(web::Data::new(app_state.clone()))
            .configure(mimotts_api::routes::configure);
        #[cfg(feature = "embed-ui")]
        if !opts.headless {
            // Single-binary UI — catch-all serves embedded assets + SPA fallback.
            return app.default_service(web::route().to(embedded_ui));
        }
        app.configure(|cfg| {
            if !opts.headless {
                if let Some(dir) = &ui_dist {
                    if dir.exists() {
                        cfg.service(
                            actix_files::Files::new("/", dir)
                                .index_file("index.html")
                                .default_handler(web::get().to(spa_fallback)),
                        );
                    }
                }
            }
        })
    })
    .workers(2)
    .bind((bind.as_str(), port))
    .map_err(|e| mimotts_engine::EngineError::Internal(format!("bind {bind}:{port}: {e}")))?;

    tracing::info!(
        "UM-MimoTTS v4 listening on http://{bind}:{port} (headless={})",
        opts.headless
    );
    server
        .run()
        .await
        .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))
}

/// SPA deep-link fallback: any unmatched GET serves index.html.
async fn spa_fallback(
    req: actix_web::HttpRequest,
    state: web::Data<mimotts_api::AppState>,
) -> actix_web::HttpResponse {
    let Some(dir) = &state.ui_dist else {
        return actix_web::HttpResponse::NotFound().body("UI not built (headless mode)");
    };
    let index = dir.join("index.html");
    match actix_files::NamedFile::open_async(&index).await {
        Ok(nf) => nf
            .set_content_type(mime_guess::from_path(&index).first_or_octet_stream())
            .into_response(&req),
        Err(e) => actix_web::HttpResponse::NotFound().body(format!("index.html missing: {e}")),
    }
}

/// Embedded-UI catch-all: asset or SPA index fallback (feature `embed-ui`).
#[cfg(feature = "embed-ui")]
async fn embedded_ui(req: actix_web::HttpRequest) -> actix_web::HttpResponse {
    use actix_web::body::BoxBody;
    let path = req.path().trim_start_matches('/');
    let target = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = mimotts_api::embedded::UiAssets::get(target) {
        let mime = mime_guess::from_path(target).first_or_octet_stream();
        return actix_web::HttpResponse::Ok()
            .content_type(mime)
            .body::<BoxBody>(BoxBody::new(file.data.into_owned()));
    }
    // SPA deep link → index.html
    if let Some(index) = mimotts_api::embedded::UiAssets::get("index.html") {
        return actix_web::HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body::<BoxBody>(BoxBody::new(index.data.into_owned()));
    }
    actix_web::HttpResponse::NotFound().body("UI assets not embedded")
}
