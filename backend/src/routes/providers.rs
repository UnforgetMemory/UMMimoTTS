//! Provider routes — list, update API key, set default.
//!
//! Providers are pre-seeded in the database (xiaomi, xiaomi-token-plan-*).
//! Users may only update the API key and choose which provider is default.

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

use super::AppState;

#[derive(Deserialize)]
pub struct UpdateApiKeyRequest {
    pub api_key: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/providers")
            .route("", web::get().to(list_providers))
            .route("/{id}", web::put().to(update_api_key))
            .route("/{id}/default", web::put().to(set_default)),
    );
}

/// GET /api/v2/providers — list all providers.
async fn list_providers(state: web::Data<AppState>) -> impl Responder {
    match state.provider_repo.find_all() {
        Ok(providers) => HttpResponse::Ok().json(providers),
        Err(e) => HttpResponse::InternalServerError().json(
            serde_json::json!({"error": e.to_string()}),
        ),
    }
}

/// PUT /api/v2/providers/{id} — update provider API key.
async fn update_api_key(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateApiKeyRequest>,
) -> impl Responder {
    let provider_id = path.into_inner();

    // Validate provider exists
    match state.provider_repo.find_by_id(&provider_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return HttpResponse::NotFound().json(
                serde_json::json!({"error": format!("Provider {provider_id} not found")}),
            );
        }
        Err(e) => {
            return HttpResponse::InternalServerError().json(
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    match state.provider_repo.update_api_key(&provider_id, &body.api_key) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Err(e) => HttpResponse::InternalServerError().json(
            serde_json::json!({"error": e.to_string()}),
        ),
    }
}

/// PUT /api/v2/providers/{id}/default — set a provider as the default.
async fn set_default(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let provider_id = path.into_inner();

    // Validate provider exists
    match state.provider_repo.find_by_id(&provider_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return HttpResponse::NotFound().json(
                serde_json::json!({"error": format!("Provider {provider_id} not found")}),
            );
        }
        Err(e) => {
            return HttpResponse::InternalServerError().json(
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    match state.provider_repo.set_default(&provider_id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Err(e) => HttpResponse::InternalServerError().json(
            serde_json::json!({"error": e.to_string()}),
        ),
    }
}
