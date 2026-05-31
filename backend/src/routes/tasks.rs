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
        task_type: format!("{:?}", task.task_type).to_lowercase(),
        status: format!("{:?}", task.status).to_lowercase(),
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
            // cancel-all must come before /{id} routes to avoid path conflict
            .route("/cancel-all", web::post().to(cancel_all_tasks))
            .route("/{id}", web::get().to(get_task))
            .route("/{id}", web::delete().to(delete_task))
            .route("/{id}/enqueue", web::post().to(enqueue_task))
            .route("/{id}/retry", web::post().to(retry_task))
            .route("/{id}/continue", web::post().to(continue_task))
            .route("/{id}/cancel", web::post().to(cancel_task))
            .route("/{id}/audio", web::get().to(get_audio))
            .route("/{id}/download", web::get().to(download_task_audio))
            .route("/{id}/title", web::patch().to(update_task_title)),
    );
    // V1 compatibility routes for frontend backward compat
    cfg.service(
        web::scope("/api/v1/tasks")
            .route("/{id}", web::get().to(get_task))
            .route("/{id}", web::delete().to(delete_task))
            .route("/{id}/audio", web::get().to(get_audio))
            .route("/{id}/download", web::get().to(download_task_audio))
            .route("/{id}/title", web::patch().to(update_task_title)),
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
    let page_size = q.page_size.unwrap_or(50).max(1).min(5000);

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

/// GET /api/v2/tasks/{id}/audio — Stream task audio file with Range support.
///
/// Returns 404 if task not found, output_path not set, or file doesn't exist on disk.
async fn get_audio(
    req: actix_web::HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    use actix_web::http::header;

    let id = path.into_inner();

    // Fetch task to get output_path
    let task = match state.task_service.get(&id) {
        Ok(Some(t)) => t,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "task not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    };

    // Check output_path exists
    let output_path = match &task.output_path {
        Some(p) => p,
        None => return HttpResponse::NotFound().json(serde_json::json!({"error": "no audio output"})),
    };

    // Read audio file from disk
    let audio_data = match std::fs::read(output_path) {
        Ok(data) => data,
        Err(_) => return HttpResponse::NotFound().json(serde_json::json!({"error": "audio file not found"})),
    };

    let audio_len = audio_data.len() as u64;

    // Parse Range header: "bytes=start-end", "bytes=start-", or "bytes=-suffix"
    let range_header = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok());

    if let Some(range_str) = range_header {
        if let Some(range_val) = range_str.strip_prefix("bytes=") {
            if let Some((start, end)) = parse_byte_range(range_val, audio_len) {
                let body = &audio_data[start as usize..=end as usize];
                let content_range = format!("bytes {}-{}/{}", start, end, audio_len);

                return HttpResponse::PartialContent()
                    .content_type("audio/wav")
                    .insert_header(("Accept-Ranges", "bytes"))
                    .insert_header(("Content-Range", content_range.as_str()))
                    .insert_header(("Content-Length", body.len().to_string()))
                    .insert_header((
                        "Content-Disposition",
                        format!("inline; filename=\"tts_{}.wav\"", id),
                    ))
                    .body(body.to_vec());
            }
        }
    }

    // No Range header or unparseable range → return full file
    HttpResponse::Ok()
        .content_type("audio/wav")
        .insert_header(("Accept-Ranges", "bytes"))
        .insert_header(("Content-Length", audio_len.to_string()))
        .insert_header((
            "Content-Disposition",
            format!("inline; filename=\"tts_{}.wav\"", id),
        ))
        .body(audio_data)
}

/// Parse a byte range string like "0-499", "500-", or "-500" into (start, end) inclusive.
fn parse_byte_range(range_val: &str, total_len: u64) -> Option<(u64, u64)> {
    if range_val.is_empty() {
        return None;
    }

    if let Some(suffix_str) = range_val.strip_prefix('-') {
        // Suffix range: "-500" means last 500 bytes
        let suffix: u64 = suffix_str.parse().ok()?;
        if suffix == 0 || suffix > total_len {
            return None;
        }
        Some((total_len - suffix, total_len - 1))
    } else if range_val.contains('-') {
        let parts: Vec<&str> = range_val.splitn(2, '-').collect();
        let start: u64 = parts[0].parse().ok()?;
        let end: u64 = if parts[1].is_empty() {
            total_len - 1
        } else {
            parts[1].parse().ok()?
        };
        if start > end || start >= total_len {
            return None;
        }
        Some((start, end.min(total_len - 1)))
    } else {
        None
    }
}

/// DELETE /api/v2/tasks/{id} — Delete a task and its audio file.
async fn delete_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();

    // Get task to find output_path for cleanup
    let task = match state.task_service.get(&id) {
        Ok(Some(t)) => t,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "task not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    };

    // Delete audio file if exists
    if let Some(path) = &task.output_path {
        let _ = std::fs::remove_file(path);
    }

    // Delete from database
    match state.task_service.delete(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

/// POST /api/v2/tasks/{id}/cancel — Cancel a single task.
async fn cancel_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.task_service.cancel(&id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"cancelled": true})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})),
    }
}

/// POST /api/v2/tasks/cancel-all — Cancel ALL non-terminal tasks.
async fn cancel_all_tasks(
    state: web::Data<AppState>,
) -> impl Responder {
    match state.batch_service.cancel_all() {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"cancelled": true})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

/// GET /api/v2/tasks/{id}/download — Download task audio as attachment.
async fn download_task_audio(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();

    let task = match state.task_service.get(&id) {
        Ok(Some(t)) => t,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "task not found"})),
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    };

    let output_path = match &task.output_path {
        Some(p) => p,
        None => return HttpResponse::NotFound().json(serde_json::json!({"error": "no audio output"})),
    };

    let audio_data = match std::fs::read(output_path) {
        Ok(data) => data,
        Err(_) => return HttpResponse::NotFound().json(serde_json::json!({"error": "audio file not found"})),
    };

    let filename = format!("tts_{}.wav", id);
    HttpResponse::Ok()
        .content_type("audio/wav")
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        ))
        .body(audio_data)
}

/// PATCH /api/v2/tasks/{id}/title — Update task title.
#[derive(serde::Deserialize)]
struct UpdateTitleRequest {
    title: String,
}

async fn update_task_title(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateTitleRequest>,
) -> impl Responder {
    let id = path.into_inner();
    match state.task_service.update_title(&id, &body.title) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}
