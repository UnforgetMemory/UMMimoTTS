use crate::models::batch::{BatchGroup, BatchCreateResponse};
use crate::models::task::TtsTask;
use crate::services::batch_queue::{BatchQueue, QueuedTask};
use crate::services::token_counter;
use crate::state::app_state::AppState;
use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use futures::StreamExt;
use serde::Deserialize;
use std::collections::HashSet;

/// Per-task configuration overrides for batch import
#[derive(Debug, Deserialize)]
struct TaskConfig {
    task_name: Option<String>,
    voice: Option<String>,
    model: Option<String>,
    context: Option<String>,
}

/// Generate task name from filename
fn generate_task_name(filename: &str, existing_names: &HashSet<String>) -> String {
    // Strip .txt extension
    let name = filename.strip_suffix(".txt").unwrap_or(filename);

    // Trim whitespace
    let name = name.trim();

    // Truncate to 64 chars
    let name = if name.len() > 64 { &name[..64] } else { name };

    // Sanitize for path compatibility (remove /\:*?"<>|)
    let sanitized: String = name
        .chars()
        .filter(|c| !"/\\:*?\"<>|".contains(*c))
        .collect();

    let name = if sanitized.is_empty() {
        "task".to_string()
    } else {
        sanitized
    };

    // Handle duplicates with sequential suffix
    if !existing_names.contains(&name) {
        return name;
    }

    let mut counter = 2;
    loop {
        let candidate = format!("{}_{}", name, counter);
        if !existing_names.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// POST /api/v1/batch/import - Import multiple TXT files as a batch group
pub async fn import_batch(
    mut payload: Multipart,
    data: web::Data<AppState>,
    queue: web::Data<BatchQueue>,
) -> impl Responder {
    let mut group_name: Option<String> = None;
    let mut voice: Option<String> = None;
    let mut model = "mimo-v2.5-tts".to_string();
    let mut context: Option<String> = None;
    let mut api_key: Option<String> = None;
    let mut task_configs_raw: Option<String> = None;
    let mut use_filename_as_task_name = true;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    // Parse multipart form data
    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "error": "文件上传失败",
                    "message": e.to_string()
                }));
            }
        };

        let name = field.name().unwrap_or("unknown").to_string();

        if let Some(content_disposition) = field.content_disposition() {
            if content_disposition.get_filename().is_some() {
                // This is a file field
                let filename = content_disposition
                    .get_filename()
                    .unwrap_or("unknown.txt")
                    .to_string();
                let mut file_data = Vec::new();

                while let Some(chunk) = field.next().await {
                    match chunk {
                        Ok(bytes) => file_data.extend_from_slice(&bytes),
                        Err(e) => {
                            return HttpResponse::BadRequest().json(serde_json::json!({
                                "error": "文件读取失败",
                                "message": e.to_string()
                            }));
                        }
                    }
                }

                files.push((filename, file_data));
            } else {
                // This is a form field
                let mut value = String::new();
                while let Some(chunk) = field.next().await {
                    if let Ok(bytes) = chunk {
                        value.push_str(&String::from_utf8_lossy(&bytes));
                    }
                }

                match name.as_str() {
                    "group_name" => group_name = Some(value),
                    "voice" => voice = Some(value),
                    "model" => model = value,
                    "context" => context = Some(value),
                    "api_key" => api_key = Some(value),
                    "task_configs" => task_configs_raw = Some(value),
                    "use_filename_as_task_name" => {
                        use_filename_as_task_name = value.trim() != "false" && value.trim() != "0";
                    }
                    _ => {}
                }
            }
        }
    }

    // Validate file count
    if files.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "没有上传文件",
            "message": "请至少上传一个TXT文件"
        }));
    }

    // Validate API key
    let api_key = match api_key {
        Some(k) if !k.trim().is_empty() => k,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "API Key 未配置",
                "message": "请先配置 API Key 再进行批量导入"
            }));
        }
    };

    // Use default group name if not provided
    let group_name = group_name.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    // Parse per-task config overrides if provided
    let task_configs: Option<Vec<TaskConfig>> = if let Some(raw) = task_configs_raw {
        match serde_json::from_str::<Vec<TaskConfig>>(&raw) {
            Ok(configs) => Some(configs),
            Err(e) => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "error": "task_configs JSON 格式错误",
                    "message": e.to_string()
                }));
            }
        }
    } else {
        None
    };

    // Create batch group
    let group = BatchGroup::new(
        group_name.clone(),
        voice.clone(),
        model.clone(),
        context.clone(),
    );
    let group_id = group.id.clone();

    // Process files and create tasks
    let mut task_ids = Vec::new();
    let mut existing_names = HashSet::new();
    let mut errors = Vec::new();

    for (i, (filename, file_data)) in files.into_iter().enumerate() {
        // Read file content as UTF-8
        let text = match String::from_utf8(file_data) {
            Ok(t) => t,
            Err(_) => {
                errors.push(format!("{}: 文件编码不是UTF-8", filename));
                continue;
            }
        };

        // Validate text not empty
        if text.trim().is_empty() {
            errors.push(format!("{}: 文件内容为空", filename));
            continue;
        }

        // Get per-task config override for this file (by index)
        let task_config = task_configs.as_ref().and_then(|configs| configs.get(i));

        // Determine task name: per-task override > filename-derived (if enabled) > auto-generated
        let task_name = if let Some(name) = task_config.and_then(|c| c.task_name.as_deref()) {
            name.to_string()
        } else if use_filename_as_task_name {
            generate_task_name(&filename, &existing_names)
        } else {
            String::new()
        };
        existing_names.insert(task_name.clone());

        // Apply per-task overrides, falling back to group defaults
        let effective_voice = task_config
            .and_then(|c| c.voice.as_deref())
            .map(|s| s.to_string())
            .or_else(|| voice.clone());
        let effective_model = task_config
            .and_then(|c| c.model.as_deref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| model.clone());
        let effective_context = task_config
            .and_then(|c| c.context.as_deref())
            .map(|s| s.to_string())
            .or_else(|| context.clone());

        // Create task
        let mut task = TtsTask::new(effective_model, effective_voice, text, effective_context);
        task.custom_title = Some(task_name);
        task.group_id = Some(group_id.clone());
        task.token_count = token_counter::count_tokens_approx(&task.text);
        task.api_key = Some(api_key.clone());

        let task_id = task.id.clone();
        data.add_task(task);
        task_ids.push(task_id.clone());

        // Enqueue to batch queue
        queue
            .enqueue(QueuedTask {
                task_id,
                group_id: Some(group_id.clone()),
                priority: 0,
                token_count: 0, // Will be set when task is processed
                enqueued_at: std::time::Instant::now(),
            })
            .await;
    }

    if task_ids.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "没有有效文件",
            "message": "所有文件都无法处理",
            "details": errors
        }));
    }

    // Update group with task IDs
    let mut group = group;
    group.task_ids = task_ids.clone();
    group.total_tasks = task_ids.len();
    data.add_group(group);

    // Return response
    HttpResponse::Ok().json(BatchCreateResponse {
        group_id,
        group_name,
        task_count: task_ids.len(),
        task_ids,
    })
}
