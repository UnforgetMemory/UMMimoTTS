//! Batch routes — CRUD + submit.

#![allow(dead_code)]

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

use super::AppState;

#[derive(Deserialize)]
pub struct CreateBatchRequest {
    pub title: String,
    #[serde(default = "default_voice")]
    pub voice: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    pub style: Option<String>,
    #[serde(default = "default_speed")]
    pub speed: f64,
}

fn default_voice() -> Option<String> {
    Some(crate::constants::DEFAULT_VOICE.to_string())
}
fn default_model() -> String {
    crate::constants::DEFAULT_MODEL.to_string()
}
fn default_speed() -> f64 {
    crate::constants::DEFAULT_SPEED
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

#[derive(Deserialize)]
pub struct UpdateBatchTitleRequest {
    pub title: String,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/batches")
            .route("", web::post().to(create_batch))
            .route("/limit", web::get().to(get_batch_limit))
            .route("/{id}", web::get().to(get_batch))
            .route("/{id}", web::put().to(update_batch))
            .route("/{id}", web::patch().to(patch_batch_title))
            .route("/{id}/items", web::post().to(add_item))
            .route("/{id}/items/batch", web::post().to(batch_add_items))
            .route("/{id}/items/{seq}", web::put().to(update_item))
            .route("/{id}/items/{seq}", web::delete().to(delete_item))
            .route("/{id}/submit", web::post().to(submit_batch))
            .route("/{id}/pause", web::post().to(pause_batch))
            .route("/{id}/cancel", web::post().to(cancel_batch))
            .route("/{id}/resume", web::post().to(resume_batch))
            .route("/{id}/retry-failed", web::post().to(retry_failed_batch))
            .route("/{id}/download", web::get().to(download_batch_audio))
            .route("/{id}", web::delete().to(delete_batch)),
    );
}

async fn create_batch(
    state: web::Data<AppState>,
    body: web::Json<CreateBatchRequest>,
) -> impl Responder {
    let voice = body.voice.clone().unwrap_or_else(|| crate::constants::DEFAULT_VOICE.to_string());
    // Advisory validation — warn but still process
    if !crate::constants::is_valid_voice(&voice) {
        tracing::warn!("Unknown voice '{}' in batch creation — will attempt synthesis anyway", voice);
    }
    if !crate::constants::is_valid_model(&body.model) {
        tracing::warn!("Unknown model '{}' in batch creation — will attempt synthesis anyway", body.model);
    }
    match state.batch_service.create(
        body.title.clone(),
        voice,
        body.model.clone(),
        body.style.clone(),
        body.speed,
    ) {
        Ok(batch) => {
            // Also create a group record so listGroups can find it
            let _ = state.group_service.create(
                &batch.id.to_string(),
                &batch.title,
                Some(batch.voice.clone()),
                Some(batch.model.clone()),
                batch.style.clone(),
                Some(batch.speed),
                None, // provider_id — Batch doesn't have a provider field yet
            );
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

// ── new endpoints ──────────────────────────────────────────────────

/// PATCH /api/v2/batches/{id} — update batch title
async fn patch_batch_title(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateBatchTitleRequest>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.update_title(&id, &body.title) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => {
            let status = match &e {
                crate::shared::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::BAD_REQUEST,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

/// POST /api/v2/batches/{id}/pause — pause batch
async fn pause_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.pause(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => {
            let status = match &e {
                crate::shared::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::BAD_REQUEST,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

/// POST /api/v2/batches/{id}/cancel — cancel batch
async fn cancel_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.cancel(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => {
            let status = match &e {
                crate::shared::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::BAD_REQUEST,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

/// POST /api/v2/batches/{id}/resume — resume batch
async fn resume_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.resume(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => {
            let status = match &e {
                crate::shared::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::BAD_REQUEST,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

/// POST /api/v2/batches/{id}/retry-failed — retry failed tasks
async fn retry_failed_batch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.retry_failed(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => {
            let status = match &e {
                crate::shared::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::BAD_REQUEST,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

/// GET /api/v2/batches/{id}/download — download batch audio as ZIP
async fn download_batch_audio(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.batch_service.download_audio(&id) {
        Ok(bytes) => HttpResponse::Ok()
            .content_type("application/zip")
            .append_header((
                actix_web::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"batch-{id}.zip\""),
            ))
            .body(bytes),
        Err(e) => {
            let status = match &e {
                crate::shared::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::BAD_REQUEST,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

/// GET /api/v2/batches/limit — get max items per batch
/// Note: This route must be registered BEFORE /{id} routes to avoid conflict.
async fn get_batch_limit() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "limit": 500,
        "max_text_length": 2000,
    }))
}
