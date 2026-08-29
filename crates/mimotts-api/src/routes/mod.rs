//! REST v3 route wiring (contract: packages/contract/openapi.yaml).

use actix_web::web;

mod auth;
mod config;
mod events;
mod import;
mod providers;
mod sessions;
mod tasks;

use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v3")
            .configure(config::configure)
            .configure(providers::configure)
            .configure(sessions::configure)
            .configure(tasks::configure)
            .configure(import::configure)
            .configure(events::configure)
            .configure(auth::configure)
            // NOTE: must live INSIDE the scope — actix scope swallows all
            // unmatched /api/v3/* paths with its own 404.
            .route("/stats", web::get().to(stats))
            // Contract alignment: openapi `servers` carries the /api/v3
            // prefix, so /health must resolve under it as well.
            .route("/health", web::get().to(health)),
    )
    // Legacy-compat alias at the root (older clients/health checks hit
    // `/health` directly; the contract path is the one above).
    .route("/health", web::get().to(health));
}

async fn stats(state: web::Data<AppState>, _auth: crate::auth::Auth) -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(state.engine.stats())
}

/// Minimal payload — no engine stats (that data requires auth via /stats).
async fn health() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
