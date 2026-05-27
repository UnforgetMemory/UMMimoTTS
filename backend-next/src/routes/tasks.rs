//! Task routes — CRUD + enqueue + retry + continue.

#![allow(dead_code)]

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

use super::AppState;

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub content: String,
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

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/tasks")
            .route("", web::post().to(create_task))
            .route("", web::get().to(list_tasks))
            .route("/{id}", web::get().to(get_task))
            .route("/{id}/enqueue", web::post().to(enqueue_task))
            .route("/{id}/retry", web::post().to(retry_task))
            .route("/{id}/continue", web::post().to(continue_task)),
    );
}

async fn create_task(
    state: web::Data<AppState>,
    body: web::Json<CreateTaskRequest>,
) -> impl Responder {
    match state.task_service.create_single(
        body.content.clone(),
        body.title.clone(),
        body.voice.clone(),
        body.model.clone(),
        body.style.clone(),
        body.speed,
    ) {
        Ok(task) => HttpResponse::Created().json(task),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn list_tasks(
    state: web::Data<AppState>,
) -> impl Responder {
    // Basic: return all tasks (no pagination yet — Phase 6+ can add)
    match state.task_service.task_repo.find_all() {
        Ok(tasks) => HttpResponse::Ok().json(tasks),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn get_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.task_service.get(&id) {
        Ok(Some(task)) => HttpResponse::Ok().json(task),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "not found"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn enqueue_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.task_service.enqueue(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn retry_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.task_service.task_repo.find_by_id(&id) {
        Ok(Some(task)) => {
            use crate::domain::task::TaskStatus;
            if task.status != TaskStatus::Failed {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "error": format!("Task status is {:?}, not Failed/FailedPermanently", task.status)
                }));
            }
            // Reset to Pending and enqueue
            if let Err(e) = state.task_service.task_repo.update_status(&id, &TaskStatus::Pending) {
                return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}));
            }
            match state.task_service.enqueue(&id).await {
                Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
                Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
            }
        }
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "not found"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

async fn continue_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.task_service.continue_task(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}
