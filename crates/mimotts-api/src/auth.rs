//! Bearer-token auth (ADR-007).
//!
//! Tokens are issued by the CLI on first run (`mimotts key issue`), stored as
//! SHA-256 hashes. The UI keeps the token in localStorage and sends
//! `Authorization: Bearer <token>`.

use actix_web::dev::Payload;
use actix_web::http::header;
use actix_web::{Error, FromRequest, HttpRequest};
use std::future::{ready, Ready};

use mimotts_engine::EngineError;

#[derive(Debug)]
pub struct Auth;

impl FromRequest for Auth {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let state = match req.app_data::<actix_web::web::Data<crate::AppState>>() {
            Some(s) => s.clone(),
            None => {
                return ready(Err(actix_web::error::ErrorUnauthorized(
                    "missing app state",
                )))
            }
        };
        let token = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.trim().to_string());
        let Some(token) = token else {
            return ready(Err(unauthorized("missing bearer token")));
        };
        match state.engine.check_token(&token) {
            Ok(true) => ready(Ok(Auth)),
            Ok(false) => ready(Err(unauthorized("invalid token"))),
            Err(e) => ready(Err(actix_web::error::ErrorInternalServerError(
                e.to_string(),
            ))),
        }
    }
}

fn unauthorized(msg: &str) -> Error {
    actix_web::error::ErrorUnauthorized(serde_json::json!({
        "error": msg,
        "code": "UNAUTHORIZED",
    }))
}

fn bearer_ok(req: &HttpRequest, state: &mimotts_engine::Engine) -> bool {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| state.check_token(t.trim()).ok())
        .unwrap_or(false)
}

/// Shared auth check for endpoints that browsers cannot easily send headers to
/// (`<audio src>`, EventSource): query `?token=` OR `Authorization: Bearer`.
pub fn token_ok(req: &HttpRequest, state: &mimotts_engine::Engine, query_token: Option<&str>) -> bool {
    let query_ok = match query_token {
        Some(t) => state.check_token(t.trim()).unwrap_or(false),
        None => false,
    };
    query_ok || bearer_ok(req, state)
}

/// Scope-aware variant: a query token may be a short-lived scoped credential
/// (must match `expected_scope`) or a full API token; with no query token the
/// bearer header is checked.
pub fn scoped_or_bearer_ok(
    req: &HttpRequest,
    state: &mimotts_engine::Engine,
    query_token: Option<&str>,
    expected_scope: &str,
) -> bool {
    match query_token {
        Some(t) if t.starts_with("scoped:") => state.check_scoped(t, expected_scope),
        Some(t) => state.check_token(t.trim()).unwrap_or(false),
        None => bearer_ok(req, state),
    }
}

/// Map engine errors to actix responses (single source of truth).
pub fn engine_error(e: EngineError) -> actix_web::HttpResponse {
    use actix_web::http::StatusCode;
    let (status, code, msg) = match &e {
        EngineError::NotFound(_) => (StatusCode::NOT_FOUND, e.code(), e.to_string()),
        EngineError::InvalidInput(_) => (StatusCode::BAD_REQUEST, e.code(), e.to_string()),
        EngineError::Conflict(_) => (StatusCode::CONFLICT, e.code(), e.to_string()),
        EngineError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, e.code(), e.to_string()),
        EngineError::RateLimited => {
            (StatusCode::TOO_MANY_REQUESTS, e.code(), e.to_string())
        }
        EngineError::ServerOverload(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, e.code(), e.to_string())
        }
        EngineError::ContentBlocked => (StatusCode::BAD_REQUEST, e.code(), e.to_string()),
        EngineError::ContextOverflow(_) => (StatusCode::BAD_REQUEST, e.code(), e.to_string()),
        EngineError::NoProvider => (StatusCode::BAD_REQUEST, e.code(), e.to_string()),
        EngineError::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.code(), e.to_string())
        }
    };
    actix_web::HttpResponse::build(status).json(serde_json::json!({
        "error": msg,
        "code": code,
    }))
}
