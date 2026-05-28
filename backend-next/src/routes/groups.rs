//! Group routes — CRUD.
//! NOTE: "groups" in the frontend are actually batches.
//! The list endpoint returns batch data mapped to GroupSummary-compatible fields.

#![allow(dead_code)]

use super::AppState;
use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub batch_id: String,
    pub title: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/groups")
            .route("", web::post().to(create_group))
            .route("", web::get().to(list_groups)),
    );
}

async fn create_group(
    state: web::Data<AppState>,
    body: web::Json<CreateGroupRequest>,
) -> impl Responder {
    match state
        .group_service
        .create(&body.batch_id, &body.title)
    {
        Ok(group) => HttpResponse::Created().json(group),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn list_groups(
    state: web::Data<AppState>,
    _query: web::Query<ListGroupsQuery>,
) -> impl Responder {
    // List batches instead of groups — the frontend's "GroupSummary" is
    // actually batch-level data (name, voice, model, tokens, etc.)
    match state.batch_service.batch_repo.list_all() {
        Ok(batches) => {
            let groups: Vec<serde_json::Value> = batches
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id.to_string(),
                        "name": b.title,
                        "status": b.status,
                        "voice": b.voice,
                        "model": b.model,
                        "context": b.style,
                        "created_at": b.created_at.to_rfc3339(),
                        "total_tasks": b.total_items,
                        "completed_tasks": 0,
                        "failed_tasks": 0,
                        "total_tokens": b.total_tokens,
                    })
                })
                .collect();
            HttpResponse::Ok().json(groups)
        }
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct ListGroupsQuery {
    pub batch_id: Option<String>,
}
