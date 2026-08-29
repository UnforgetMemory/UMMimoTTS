//! /api/v3/providers — list / set key / set default.

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::engine_error;
use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/providers")
            .route("", web::get().to(list))
            .route("/{id}", web::put().to(edit))
            .route("/{id}/key", web::put().to(set_key))
            .route("/{id}/default", web::put().to(set_default)),
    );
}

#[derive(Deserialize)]
struct EditBody {
    name: Option<String>,
    base_url: Option<String>,
    budget_group: Option<String>,
}

/// Edit provider metadata (name / base_url / budget_group) — custom upstreams.
async fn edit(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
    body: web::Json<EditBody>,
) -> HttpResponse {
    if body.name.is_none() && body.base_url.is_none() && body.budget_group.is_none() {
        return engine_error(mimotts_engine::EngineError::InvalidInput(
            "nothing to edit".into(),
        ));
    }
    match state.engine.edit_provider(
        &path,
        body.name.as_deref(),
        body.base_url.as_deref(),
        body.budget_group.as_deref(),
    ) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Err(e) => engine_error(e),
    }
}

async fn list(state: web::Data<AppState>, _auth: crate::auth::Auth) -> HttpResponse {
    match state.engine.providers() {
        Ok(p) => HttpResponse::Ok().json(p),
        Err(e) => engine_error(e),
    }
}

#[derive(Deserialize)]
struct KeyBody {
    api_key: String,
}

async fn set_key(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
    body: web::Json<KeyBody>,
) -> HttpResponse {
    match state.engine.set_provider_key(&path, &body.api_key) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Err(e) => engine_error(e),
    }
}

async fn set_default(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.set_default_provider(&path) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Err(e) => engine_error(e),
    }
}
