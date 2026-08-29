//! GET /api/v3/config — voices/models/providers/defaults/chunk settings.
//! GET /api/v3/voices/{id}/preview — whitelisted 302 proxy to official CDN samples.

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use mimotts_core::catalog::{DEFAULT_MODEL, DEFAULT_VOICE, MODELS, VOICES};

use crate::auth::engine_error;
use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/config", web::get().to(get_config))
        .route("/voices/{id}/preview", web::get().to(preview));
}

/// Only the official sample CDN may be proxied (SSRF guard).
const PREVIEW_ALLOWLIST: &str = "https://aistudio-cdn.xiaomimimo.com/";

#[derive(Deserialize)]
struct PreviewQuery {
    token: Option<String>,
}

async fn preview(
    req: actix_web::HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    q: web::Query<PreviewQuery>,
) -> HttpResponse {
    let scope = format!("preview:{}", path.as_str());
    if !crate::auth::scoped_or_bearer_ok(&req, &state.engine, q.token.as_deref(), &scope) {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "missing or invalid token", "code": "UNAUTHORIZED",
        }));
    }
    let preset = VOICES.iter().find(|v| v.id == path.as_str());
    match preset.and_then(|v| v.preview_url) {
        Some(url) if url.starts_with(PREVIEW_ALLOWLIST) => HttpResponse::Found()
            .insert_header(("Location", url))
            .finish(),
        Some(_) => engine_error(mimotts_engine::EngineError::Internal(
            "preview url outside allowlist".into(),
        )),
        None => engine_error(mimotts_engine::EngineError::NotFound(format!(
            "voice {} has no preview",
            path
        ))),
    }
}

async fn get_config(state: web::Data<AppState>, _auth: crate::auth::Auth) -> HttpResponse {
    let providers = match state.engine.providers() {
        Ok(p) => p,
        Err(e) => return engine_error(e),
    };
    let chunk = state.engine.config();
    HttpResponse::Ok().json(serde_json::json!({
        "voices": VOICES,
        "models": MODELS,
        "providers": providers,
        "default_voice": DEFAULT_VOICE,
        "default_model": DEFAULT_MODEL,
        "chunk": {
            "context_window_tokens": chunk.context_window_tokens,
            "target_tokens": chunk.chunk_target_tokens,
            "hard_cap_tokens": chunk.chunk_hard_cap_tokens,
        },
        "announcement": chunk.announcement,
        "stream_audio": chunk.stream_audio,
    }))
}
