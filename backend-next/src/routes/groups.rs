//! Group routes — CRUD.

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
    query: web::Query<ListGroupsQuery>,
) -> impl Responder {
    match state.group_service.list(query.batch_id.as_deref()) {
        Ok(groups) => HttpResponse::Ok().json(groups),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct ListGroupsQuery {
    pub batch_id: Option<String>,
}
