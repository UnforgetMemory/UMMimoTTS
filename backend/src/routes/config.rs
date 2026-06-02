//! Config endpoint — exposes voice presets, model registry, and defaults.

use actix_web::{web, HttpResponse};

use crate::constants;

/// GET /api/v2/config — returns all voice/model presets and defaults.
async fn get_config() -> HttpResponse {
    // Use MIMO_BASE_URL from env if set, otherwise compile-time default.
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| constants::MIMO_BASE_URL_DEFAULT.to_string());

    HttpResponse::Ok().json(constants::config_response(&base_url))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/api/v2/config").route(web::get().to(get_config)));
}
