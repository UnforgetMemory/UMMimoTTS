//! Task routes — CRUD + enqueue + retry + continue.

#![allow(dead_code)]

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize)]
pub struct ListTasksQuery {
    pub batch_id: Option<String>,
    pub group_id: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// Lightweight task response — NO content field (avoids multi-MB payloads).
#[derive(Serialize)]
struct TaskListItem {
    pub id: String,
    pub task_type: String,
    pub status: String,
    pub batch_id: Option<String>,
    pub group_id: Option<String>,
    pub title: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub speed: f64,
    pub total_chars: i64,
    pub total_tokens: i64,
    pub total_chunks: i32,
    pub done_chunks: i32,
    pub failed_chunks: i32,
    pub output_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Serialize)]
struct PaginatedTasksResponse {
    pub data: Vec<TaskListItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

fn default_model() -> String {
    "tts-1".to_string()
}
fn default_speed() -> f64 {
    1.0
}

/// Convert a Task to a lightweight list item (no content).
fn to_list_item(task: &crate::domain::task::Task) -> TaskListItem {
    TaskListItem {
        id: task.id.to_string(),
        task_type: format!("{:?}", task.task_type),
        status: format!("{:?}", task.status),
        batch_id: task.batch_id.as_ref().map(|id| id.to_string()),
        group_id: task.group_id.as_ref().map(|id| id.to_string()),
        title: task.title.clone(),
        voice: task.voice.clone(),
        model: task.model.clone(),
        style: task.style.clone(),
        speed: task.speed,
        total_chars: task.total_chars,
        total_tokens: task.total_tokens,
        total_chunks: task.total_chunks,
        done_chunks: task.done_chunks,
        failed_chunks: task.failed_chunks,
        output_path: task.output_path.clone(),
        created_at: task.created_at.to_rfc3339(),
        updated_at: task.updated_at.to_rfc3339(),
        completed_at: task.completed_at.map(|dt| dt.to_rfc3339()),
    }
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
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
    query: web::Query<ListTasksQuery>,
) -> impl Responder {
    let q = query.into_inner();
    let page = q.page.unwrap_or(0).max(0);
    let page_size = q.page_size.unwrap_or(50).max(1).min(200);

    // Fetch tasks filtered by batch_id or group_id if provided
    let all: Vec<crate::domain::task::Task> = if let Some(bid) = &q.batch_id {
        match state.task_service.get_by_batch(bid) {
            Ok(t) => t,
            Err(e) => return HttpResponse::InternalServerError().json(
                serde_json::json!({"error": e.to_string()})
            ),
        }
    } else if let Some(gid) = &q.group_id {
        // Filter tasks by group_id using dedicated query
        match state.task_service.task_repo.find_by_group(gid) {
            Ok(t) => t,
            Err(e) => return HttpResponse::InternalServerError().json(
                serde_json::json!({"error": e.to_string()})
            ),
        }
    } else {
        match state.task_service.task_repo.find_all() {
            Ok(t) => t,
            Err(e) => return HttpResponse::InternalServerError().json(
                serde_json::json!({"error": e.to_string()})
            ),
        }
    };

    let total = all.len() as i64;
    let start = (page * page_size) as usize;
    let items: Vec<TaskListItem> = all
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .map(|t| to_list_item(&t))
        .collect();

    HttpResponse::Ok().json(PaginatedTasksResponse {
        data: items,
        total,
        page,
        page_size,
    })
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
    match state.task_service.retry(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
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
