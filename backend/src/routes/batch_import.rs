use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use futures::StreamExt;
use serde::Deserialize;
use std::collections::BTreeMap;
use crate::models::batch_import::ParsedItem;
use crate::models::batch::BatchGroup;
use crate::models::task::{TaskStatus, TtsTask};
use crate::models::response::PaginatedResponse;
use crate::services::batch_queue::{BatchQueue, QueuedTask};
use crate::state::app_state::AppState;
use uuid::Uuid;

/// Parse content lines into ParsedItems, detecting `# filename.txt` file boundary markers.
fn parse_items_from_content(content: &str, filename: &str) -> Vec<ParsedItem> {
    let mut items = Vec::new();
    let mut current_filename = filename.to_string();
    let mut index = 0usize;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if this is a file header line (# filename.txt)
        if trimmed.starts_with("# ") {
            let rest = trimmed[2..].trim();
            // Treat lines ending with .txt (or common extensions) as file boundary markers
            if rest.ends_with(".txt") || rest.ends_with(".TXT") || rest.ends_with(".json") || rest.ends_with(".csv") {
                current_filename = rest.to_string();
                continue; // Skip the header line itself
            }
        }

        // Try JSON first, fallback to plain text
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let text = obj.get("text").and_then(|v| v.as_str()).unwrap_or(trimmed).to_string();
            let char_count = text.chars().count();
            let token_count = (char_count as f64 / 1.5).ceil() as usize;
            items.push(ParsedItem {
                index,
                text,
                voice: obj.get("voice").and_then(|v| v.as_str()).map(|s| s.to_string()),
                model: obj.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()),
                title: obj.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
                context: obj.get("context").and_then(|v| v.as_str()).map(|s| s.to_string()),
                speed: obj.get("speed").and_then(|v| v.as_f64()).map(|f| f as f32),
                error: None,
                source_filename: Some(current_filename.clone()),
                token_count,
            });
        } else {
            let char_count = trimmed.chars().count();
            let token_count = (char_count as f64 / 1.5).ceil() as usize;
            items.push(ParsedItem {
                index,
                text: trimmed.to_string(),
                voice: None,
                model: None,
                title: None,
                context: None,
                speed: None,
                error: None,
                source_filename: Some(current_filename.clone()),
                token_count,
            });
        }
        index += 1;
    }
    items
}

/// POST /api/v1/batch/upload — upload a file and parse items
#[derive(Deserialize)]
pub struct UploadQuery {
    pub token: Option<String>, // for appending to existing import
}

pub async fn upload_file(
    query: web::Query<UploadQuery>,
    data: web::Data<AppState>,
    mut payload: Multipart,
) -> impl Responder {
    let mut filename = String::new();
    let mut file_content = Vec::new();

    while let Some(Ok(mut field)) = payload.next().await {
        if let Some(cd) = field.content_disposition() {
            filename = cd.get_filename().unwrap_or("batch.txt").to_string();
        }
        while let Some(Ok(chunk)) = field.next().await {
            file_content.extend_from_slice(&chunk);
        }
    }

    if file_content.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "No file uploaded"}));
    }

    // Parse content using parse_items_from_content to handle # filename.txt headers
    let content = String::from_utf8_lossy(&file_content);
    let items = parse_items_from_content(&content, &filename);

    let token = data.batch_imports.create_import(filename.clone(), items);

    // Build response with stats and file_stats
    let import_data = data.batch_imports.get_import(&token);
    HttpResponse::Ok().json(serde_json::json!({
        "token": token,
        "stats": import_data.as_ref().map(|i| i.stats()),
        "file_stats": import_data.map(|i| i.file_stats()),
    }))
}

/// GET /api/v1/batch/preview?token=xxx&page=0&per_page=50
#[derive(Deserialize)]
pub struct PreviewQuery {
    pub token: String,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

pub async fn get_preview(
    query: web::Query<PreviewQuery>,
    data: web::Data<AppState>,
) -> impl Responder {
    let page = query.page.unwrap_or(0);
    let per_page = query.per_page.unwrap_or(50).min(200);

    match data.batch_imports.get_preview(&query.token, page, per_page) {
        Some((items, total)) => {
            let resp = PaginatedResponse::new(items, total, page, per_page);
            HttpResponse::Ok().json(resp)
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Import not found or expired"
        })),
    }
}

/// POST /api/v1/batch/extend — extend TTL
#[derive(Deserialize)]
pub struct ExtendBody {
    pub token: String,
}

pub async fn extend_ttl(
    body: web::Json<ExtendBody>,
    data: web::Data<AppState>,
) -> impl Responder {
    if data.batch_imports.extend_ttl(&body.token) {
        HttpResponse::Ok().json(serde_json::json!({"status": "extended"}))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({
            "error": "Import not found or expired"
        }))
    }
}

/// PUT /api/v1/batch/items/{index} — update item overrides
#[derive(Deserialize)]
pub struct UpdateItemBody {
    pub token: String,
    pub voice: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
}

pub async fn update_item(
    path: web::Path<usize>,
    body: web::Json<UpdateItemBody>,
    data: web::Data<AppState>,
) -> impl Responder {
    let index = path.into_inner();
    match data.batch_imports.update_item(
        &body.token,
        index,
        body.voice.clone(),
        body.model.clone(),
        body.title.clone(),
        body.context.clone(),
    ) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "updated"})),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e})),
    }
}

/// POST /api/v1/batch/submit — create group + tasks + enqueue
#[derive(Deserialize)]
pub struct SubmitBody {
    pub token: String,
    pub group_name: Option<String>,
    pub default_voice: Option<String>,
    pub default_model: Option<String>,
    pub default_context: Option<String>,
    pub default_speed: Option<f32>,
}

pub async fn submit(
    body: web::Json<SubmitBody>,
    data: web::Data<AppState>,
    queue: web::Data<BatchQueue>,
) -> impl Responder {
    // Get and validate import
    let import = match data.batch_imports.get_import(&body.token) {
        Some(i) => i,
        None => return HttpResponse::NotFound().json(serde_json::json!({
            "error": "Import not found or expired"
        })),
    };

    if import.submitted {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Import already submitted"
        }));
    }

    // Mark as submitted (prevent double-submit)
    data.batch_imports.mark_submitted(&body.token);

    // Create batch group
    let group_id = Uuid::new_v4().to_string();
    let group_name = body.group_name.clone().unwrap_or_else(|| import.original_filename.clone());
    let mut group = BatchGroup::new(
        group_name,
        body.default_voice.clone(),
        body.default_model.clone().unwrap_or_else(|| "default".to_string()),
        body.default_context.clone(),
    );
    group.id = group_id.clone();

    // Group valid items by source_filename — one TtsTask per file
    let valid_items: Vec<&ParsedItem> = import.items.iter().filter(|i| i.error.is_none()).collect();
    let mut file_groups: BTreeMap<String, Vec<&ParsedItem>> = BTreeMap::new();
    for item in &valid_items {
        let key = item.source_filename.clone().unwrap_or_else(|| import.original_filename.clone());
        file_groups.entry(key).or_default().push(item);
    }

    let mut task_ids = Vec::with_capacity(file_groups.len());
    let default_model = body.default_model.clone().unwrap_or_else(|| "default".to_string());

    for (filename, items) in &file_groups {
        let task_id = Uuid::new_v4().to_string();
        let full_text: Vec<String> = items.iter().map(|i| i.text.clone()).collect();
        let char_count: usize = full_text.iter().map(|s| s.len()).sum();
        let full_text = full_text.join("\n");

        // Use the first item's overrides if set, otherwise fall back to group defaults
        let first = items.first().copied();

        let task = TtsTask {
            id: task_id.clone(),
            custom_title: Some(filename.clone()),
            status: TaskStatus::Pending,
            model: first.and_then(|i| i.model.clone()).unwrap_or_else(|| default_model.clone()),
            voice: first.and_then(|i| i.voice.clone()).or_else(|| body.default_voice.clone()),
            text: full_text,
            context: first.and_then(|i| i.context.clone()).or_else(|| body.default_context.clone()),
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            audio_data: None,
            error: None,
            progress: 0.0,
            token_count: 0,
            char_count,
            audio_duration_secs: None,
            audio_path: None,
            total_chunks: None,
            current_chunk: None,
            group_id: Some(group_id.clone()),
            api_key: None,
        };

        data.add_task(task);
        task_ids.push(task_id.clone());

        // Enqueue to batch queue for processing
        queue
            .enqueue(QueuedTask {
                task_id,
                group_id: Some(group_id.clone()),
                priority: 0,
                token_count: 0,
                enqueued_at: std::time::Instant::now(),
            })
            .await;
    }

    // Update group with task IDs
    group.task_ids = task_ids.clone();
    data.update_group(&group_id, |g| {
        g.task_ids = task_ids.clone();
    });

    HttpResponse::Ok().json(serde_json::json!({
        "group_id": group_id,
        "task_count": task_ids.len(),
        "task_ids": task_ids,
    }))
}

/// GET /api/v1/batch/files?token=xxx&sort=filename&dir=asc&page=0&per_page=20
#[derive(Deserialize)]
pub struct FileStatsQuery {
    pub token: String,
    pub sort: Option<String>,
    pub dir: Option<String>,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

pub async fn get_file_stats(
    query: web::Query<FileStatsQuery>,
    data: web::Data<AppState>,
) -> impl Responder {
    let page = query.page.unwrap_or(0);
    let per_page = query.per_page.unwrap_or(20).min(100);
    let sort = query.sort.as_deref().unwrap_or("filename");
    let dir = query.dir.as_deref().unwrap_or("asc");

    match data.batch_imports.get_file_stats(&query.token, sort, dir, page, per_page) {
        Some((file_stats, total)) => {
            let resp = PaginatedResponse::new(file_stats, total, page, per_page);
            HttpResponse::Ok().json(resp)
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Import not found or expired"
        })),
    }
}

/// DELETE /api/v1/batch/files/{filename}?token=xxx
#[derive(Deserialize)]
pub struct DeleteFileQuery {
    pub token: String,
}

pub async fn delete_file(
    path: web::Path<String>,
    query: web::Query<DeleteFileQuery>,
    data: web::Data<AppState>,
) -> impl Responder {
    let filename = path.into_inner();
    match data.batch_imports.remove_file(&query.token, &filename) {
        Ok(removed_count) => HttpResponse::Ok().json(serde_json::json!({
            "removed_count": removed_count,
        })),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
            "error": e
        })),
    }
}
