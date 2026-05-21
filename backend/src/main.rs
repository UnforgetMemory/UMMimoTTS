use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use tracing_subscriber;

mod config;
mod embed;
mod models;
mod routes;
mod services;
mod state;

use config::Config;
use state::app_state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mimo_tts_server=info".parse().unwrap())
                .add_directive("actix_web=info".parse().unwrap()),
        )
        .init();

    let config = Config::from_env();
    let app_state = web::Data::new(AppState::new());

    tracing::info!("Starting MIMO TTS Server on port {}", config.server_port);
    tracing::info!("Allowed origins: {:?}", config.allowed_origins);

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _req_head| {
                origin.as_bytes().starts_with(b"http://localhost:")
            })
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
                actix_web::http::header::CONTENT_TYPE,
                actix_web::http::header::RANGE,
            ])
            .expose_headers(vec![
                actix_web::http::header::ACCEPT_RANGES,
                actix_web::http::header::CONTENT_RANGE,
                actix_web::http::header::CONTENT_LENGTH,
            ])
            .max_age(3600);

        App::new()
            .app_data(app_state.clone())
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .route(
                "/api/v1/tts/synthesize",
                web::post().to(routes::tts::synthesize),
            )
            .route("/api/v1/tasks", web::get().to(routes::tasks::list_tasks))
            .route(
                "/api/v1/tasks/{task_id}",
                web::get().to(routes::tasks::get_task),
            )
            .route(
                "/api/v1/tasks/{task_id}",
                web::delete().to(routes::tasks::delete_task),
            )
            .route(
                "/api/v1/tasks/{task_id}/audio",
                web::get().to(routes::tasks::get_audio),
            )
            .route(
                "/api/v1/tasks/{task_id}/title",
                web::patch().to(routes::tasks::update_task_title),
            )
            .route("/api/v1/voices", web::get().to(routes::voices::list_voices))
            .route(
                "/api/v1/voices/{voice_id}/preview",
                web::get().to(routes::voices::preview_voice),
            )
            .route(
                "/api/v1/sse/tasks/{task_id}",
                web::get().to(routes::sse::sse_task_events),
            )
            .route(
                "/health",
                web::get().to(|| async {
                    actix_web::HttpResponse::Ok().json(serde_json::json!({
                        "status": "ok",
                        "version": env!("CARGO_PKG_VERSION"),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }))
                }),
            )
            // 嵌入的前端静态文件（通配路由放在最后）
            .configure(embed::config_embedded)
    })
    .bind(("0.0.0.0", config.server_port))?
    .run();

    tracing::info!("Server started at http://0.0.0.0:{}", config.server_port);

    server.await
}
