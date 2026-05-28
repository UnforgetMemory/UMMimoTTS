//! Batch routes — CRUD + submit.

#![allow(dead_code)]

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

use super::AppState;

#[derive(Deserialize)]
pub struct CreateBatchRequest {
    pub title: String,
    pub voice: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub style: Option<String>,
    #[serde(default = "default_speed")]
    pub speed: f64,
}

fn default_model() -> String {
    "tts-1".to_string()
}
fn default_speed() -> f64 {
    1.0
}

#[derive(Deserialize)]
pub struct AddItemRequest {
    pub seq: i32,
    pub filename: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct UpdateItemRequest {
    pub voice: Option<String>,
    pub model: Option<String>,
    pub style: Option<String>,
    pub speed: Option<f64>,
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateBatchStatusRequest {
    pub status: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/batches")
            .route("", web::post().to(create_batch))
            .route("/{id}", web::get().to(get_batch))
            .route("/{id}", web::put().to(update_batch))
            .route("/{id}/items", web::post().to(add_item))
            .route("/{id}/items/batch", web::post().to(batch_add_items))
            .route("/{id}/items/{seq}", web::put().to(update_item))
            .route("/{id}/items/{seq}", web::delete().to(delete_item))
            .route("/{id}/submit", web::post().to(submit_batch))
            .route("/{id}", web::delete().to(delete_batch)),
    );
}

async fn create_batch(
    state: web::Data<AppState>,
    body: web::Json<CreateBatchRequest>,
) -> impl Responder {
    match state.batch_service.create(
        body.title.clone(),
        body.voice.clone(),
        body.model.clone(),
        body.style.clone(),
        body.speed,
    ) {
        Ok(batch) => {
            // Also create a group record so listGroups can find it
            let _ = state.group_service.create(&batch.id.to_string(), &batch.title);
            HttpResponse::Ok().json(batch)
        },
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn get_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.batch_repo.find_batch(&id) {
        Ok(Some(batch)) => HttpResponse::Ok().json(batch),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "not found"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn update_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateBatchStatusRequest>,
) -> impl Responder {
    let id = path.into_inner();
    // Parse status string into BatchStatus
    let status: Result<crate::domain::batch::BatchStatus, _> =
        serde_json::from_str(&format!("\"{}\"", body.status));
    match status {
        Ok(bs) => match state.batch_service.batch_repo.update_batch_status(&id, &bs) {
            Ok(()) => HttpResponse::Ok().json(serde_json::json!({"updated": true})),
            Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
        },
        Err(_) => HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Invalid status: {}", body.status)
        })),
    }
}

async fn add_item(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<AddItemRequest>,
) -> impl Responder {
    let id = path.into_inner();
    match state
        .batch_service
        .add_item(&id, body.seq, &body.filename, &body.content)
    {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn batch_add_items(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<Vec<AddItemRequest>>,
) -> impl Responder {
    let id = path.into_inner();
    let items = body.into_inner();
    match state
        .batch_service
        .add_items(&id, &items)
    {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"ok": true, "count": items.len()})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn update_item(
    state: web::Data<AppState>,
    path: web::Path<(String, i32)>,
    body: web::Json<UpdateItemRequest>,
) -> impl Responder {
    let (batch_id, seq) = path.into_inner();
    use crate::infra::persistence::batch_repo::ItemOverride;
    let override_data = ItemOverride {
        voice: body.voice.clone(),
        model: body.model.clone(),
        style: body.style.clone(),
        speed: body.speed,
        title: body.title.clone(),
    };
    match state.batch_service.update_item(&batch_id, seq, &override_data) {
        Ok(_item) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn delete_item(
    state: web::Data<AppState>,
    path: web::Path<(String, i32)>,
) -> impl Responder {
    let (batch_id, seq) = path.into_inner();
    match state.batch_service.batch_repo.find_pending_item_by_seq(&batch_id, seq) {
        Ok(Some(item)) => {
            match state.batch_service.batch_repo.delete_pending_item(&item.id) {
                Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
                Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
            }
        }
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "item not found"})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn submit_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.submit(&id).await {
        Ok(tasks) => HttpResponse::Ok().json(tasks),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn delete_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.delete(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}
