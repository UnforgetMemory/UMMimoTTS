//! Group routes — CRUD.
//! NOTE: "groups" in the frontend are actually batches.
//! The list endpoint returns batch data mapped to GroupSummary-compatible fields.

#![allow(dead_code)]

use super::AppState;
use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub batch_ids: Vec<String>,
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
    let mut groups = Vec::new();
    for batch_id in &body.batch_ids {
        match state.group_service.create(
            batch_id,
            &body.name,
            None,
            None,
            None,
            None,
            None,
        ) {
            Ok(group) => groups.push(group),
            Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
        }
    }
    HttpResponse::Created().json(groups)
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
                    // Get task counts for this batch
                    let (completed, failed) = match state.task_service.get_by_batch(&b.id.to_string()) {
                        Ok(tasks) => {
                            let completed = tasks.iter().filter(|t| t.status == crate::domain::task::TaskStatus::Done).count() as i32;
                            let failed = tasks.iter().filter(|t| t.status == crate::domain::task::TaskStatus::Failed).count() as i32;
                            (completed, failed)
                        }
                        Err(_) => (0, 0),
                    };
                    // Calculate progress
                    let progress = if b.total_items > 0 {
                        (completed as f64 / b.total_items as f64 * 100.0).round() as i32
                    } else {
                        0
                    };
                    serde_json::json!({
                        "id": b.id.to_string(),
                        "name": b.title,
                        "status": b.status,
                        "voice": b.voice,
                        "model": b.model,
                        "context": b.style,
                        "created_at": b.created_at.to_rfc3339(),
                        "total_tasks": b.total_items,
                        "completed_tasks": completed,
                        "failed_tasks": failed,
                        "total_tokens": b.total_tokens,
                        "progress": progress,
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
