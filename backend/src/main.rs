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

/// 查找可用端口，如果指定端口被占用则自动递增
fn find_available_port(preferred: u16) -> u16 {
    if std::net::TcpListener::bind(("127.0.0.1", preferred)).is_ok() {
        return preferred;
    }
    find_next_port(preferred)
}

/// 查找下一个可用端口
fn find_next_port(from: u16) -> u16 {
    for offset in 1..=100 {
        let port = from.wrapping_add(offset);
        if port >= 1024 && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    for port in 10000..65535 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    tracing::error!("No available port found!");
    std::process::exit(1);
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("um_mimo_tts_server=info".parse().unwrap())
                .add_directive("actix_web=info".parse().unwrap()),
        )
        .init();

    let mut config = Config::from_args();
    let app_state = web::Data::new(AppState::new());

    // 尝试绑定端口，如果被占用则自动查找可用端口
    let port = find_available_port(config.server_port);
    config.server_port = port;

    // 启动提示
    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║           UM-MIMO-TTS Server v{}                   ║", env!("CARGO_PKG_VERSION"));
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  🌐 Web UI:  http://localhost:{:<5}                    ║", config.server_port);
    println!("║  📡 API:     http://localhost:{:<5}/api/v1             ║", config.server_port);
    println!("║  ❤️  Health:  http://localhost:{:<5}/health             ║", config.server_port);
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  按 Ctrl+C 停止服务器                                      ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    tracing::info!("Starting UM-MIMO-TTS Server on port {}", config.server_port);
    tracing::info!("Allowed origins: {:?}", config.allowed_origins);

    // 尝试绑定端口，失败则自动查找可用端口
    let mut bind_port = config.server_port;
    let server = loop {
        let app_state_clone = app_state.clone();
        match HttpServer::new(move || {
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
                .app_data(app_state_clone.clone())
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
        .bind(("0.0.0.0", bind_port))
        {
            Ok(server) => break server,
            Err(e) => {
                tracing::warn!("Port {} bind failed: {}, trying next port...", bind_port, e);
                bind_port = find_next_port(bind_port);
                config.server_port = bind_port;
                // 重新打印启动提示
                println!("╔════════════════════════════════════════════════════════════╗");
                println!("║           UM-MIMO-TTS Server v{}                   ║", env!("CARGO_PKG_VERSION"));
                println!("╠════════════════════════════════════════════════════════════╣");
                println!("║  🌐 Web UI:  http://localhost:{:<5}                    ║", bind_port);
                println!("║  📡 API:     http://localhost:{:<5}/api/v1             ║", bind_port);
                println!("║  ❤️  Health:  http://localhost:{:<5}/health             ║", bind_port);
                println!("╠════════════════════════════════════════════════════════════╣");
                println!("║  按 Ctrl+C 停止服务器                                      ║");
                println!("╚════════════════════════════════════════════════════════════╝");
                println!();
            }
        }
    };

    tracing::info!("Server started at http://0.0.0.0:{}", bind_port);

    server.run().await
}
