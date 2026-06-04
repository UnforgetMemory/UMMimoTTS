//! Config endpoint — exposes voice presets, model registry, providers, and defaults.

use actix_web::{web, HttpResponse};

use crate::constants;
use super::AppState;

/// GET /api/v2/config — returns all voice/model presets, providers, and defaults.
async fn get_config(state: web::Data<AppState>) -> HttpResponse {
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| constants::MIMO_BASE_URL_DEFAULT.to_string());

    let providers = state.provider_repo.find_all().unwrap_or_default();

    HttpResponse::Ok().json(serde_json::json!({
        "voices": constants::VOICE_PRESETS,
        "models": constants::MODEL_PRESETS,
        "providers": providers,
        "default_voice": constants::DEFAULT_VOICE,
        "default_model": constants::DEFAULT_MODEL,
        "default_speed": constants::DEFAULT_SPEED,
        "mimo_base_url": base_url,
    }))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/api/v2/config").route(web::get().to(get_config)));
}
