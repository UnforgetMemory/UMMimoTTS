//! POST /api/v3/auth/scoped — issue a short-lived scoped credential (umreview B).
//!
//! Raw bearer tokens never ride in URLs; media/SSE endpoints accept a
//! `scoped:` credential signed with the local master key instead.

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::engine_error;
use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/auth/scoped", web::post().to(issue));
}

#[derive(Deserialize)]
struct ScopedBody {
    scope: String,
}

const DEFAULT_TTL_SECS: u64 = 300;

async fn issue(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    body: web::Json<ScopedBody>,
) -> HttpResponse {
    match state.engine.issue_scoped_token(&body.scope, DEFAULT_TTL_SECS) {
        Ok(token) => HttpResponse::Ok().json(serde_json::json!({
            "token": token,
            "expires_in": DEFAULT_TTL_SECS,
            "scope": body.scope,
        })),
        Err(e) => engine_error(e),
    }
}
