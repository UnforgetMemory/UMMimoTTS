use crate::models::batch::{GroupUpdateRequest};
use crate::models::response::{ErrorResponse, GroupSummary, PaginatedResponse, TaskSummary};
use crate::state::app_state::AppState;
use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use std::io::Write;

/// Query params for group task listing
#[derive(Debug, Deserialize)]
pub struct GroupTaskQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

/// GET /api/v1/groups - List all groups (paginated, no tasks embedded)
pub async fn list_groups(
    query: web::Query<GroupTaskQuery>,
    data: web::Data<AppState>,
) -> impl Responder {
    let page = query.page.unwrap_or(0);
    let per_page = query.per_page.unwrap_or(20).min(100);

    let (groups, total) = data.list_groups_paginated(page, per_page);
    let summaries: Vec<GroupSummary> = groups.into_iter().map(|g| g.to_summary()).collect();
    let response = PaginatedResponse::new(summaries, total, page, per_page);

    HttpResponse::Ok().json(response)
}

/// GET /api/v1/groups/{group_id} - Get group details (no embedded tasks)
pub async fn get_group(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    match data.get_group(&group_id) {
        Some(g) => {
            HttpResponse::Ok().json(g.to_summary())
        }
        None => HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        }),
    }
}

/// GET /api/v1/groups/{group_id}/tasks - List tasks in a group (paginated)
pub async fn get_group_tasks(
    path: web::Path<String>,
    query: web::Query<GroupTaskQuery>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();
    let page = query.page.unwrap_or(0);
    let per_page = query.per_page.unwrap_or(50).min(200);

    // Verify group exists
    if data.get_group(&group_id).is_none() {
        return HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        });
    }

    let (tasks, total) = data.list_group_tasks_paginated(&group_id, page, per_page);
    let summaries: Vec<TaskSummary> = tasks.into_iter().map(|t| t.to_summary()).collect();
    let response = PaginatedResponse::new(summaries, total, page, per_page);

    HttpResponse::Ok().json(response)
}

/// DELETE /api/v1/groups/{group_id} - Delete group and its tasks
pub async fn delete_group(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    match data.remove_group(&group_id) {
        Some(group) => {
            // Delete all tasks in the group
            for task_id in &group.task_ids {
                data.remove_task(task_id);
            }

            HttpResponse::Ok().json(serde_json::json!({
                "message": "分组已删除",
                "deleted_tasks": group.task_ids.len()
            }))
        }
        None => HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        }),
    }
}

/// PATCH /api/v1/groups/{group_id} - Update group settings
pub async fn update_group(
    path: web::Path<String>,
    body: web::Json<GroupUpdateRequest>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    let updated = data.update_group(&group_id, |group| {
        if let Some(name) = &body.name {
            group.name = name.clone();
        }
        if let Some(voice) = &body.voice {
            group.voice = Some(voice.clone());
        }
        if let Some(context) = &body.context {
            group.context = Some(context.clone());
        }
    });

    if updated {
        HttpResponse::Ok().json(serde_json::json!({
            "message": "分组已更新"
        }))
    } else {
        HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        })
    }
}

/// POST /api/v1/groups/{group_id}/pause - Pause group processing
pub async fn pause_group(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    let updated = data.update_group(&group_id, |group| {
        group.status = crate::models::batch::GroupStatus::Paused;
    });

    if updated {
        HttpResponse::Ok().json(serde_json::json!({
            "message": "分组已暂停"
        }))
    } else {
        HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        })
    }
}

/// POST /api/v1/groups/{group_id}/resume - Resume group processing
pub async fn resume_group(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    let updated = data.update_group(&group_id, |group| {
        group.status = crate::models::batch::GroupStatus::Processing;
    });

    if updated {
        HttpResponse::Ok().json(serde_json::json!({
            "message": "分组已恢复"
        }))
    } else {
        HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        })
    }
}

/// POST /api/v1/groups/{group_id}/retry-failed - Retry failed tasks in group
pub async fn retry_failed(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    let group = match data.get_group(&group_id) {
        Some(g) => g,
        None => {
            return HttpResponse::NotFound().json(ErrorResponse {
                error: "分组不存在".to_string(),
                message: format!("分组 ID: {}", group_id),
                code: Some("GROUP_NOT_FOUND".to_string()),
            });
        }
    };

    // Find failed tasks and reset them
    let mut retried_count = 0;
    for task_id in &group.task_ids {
        if let Some(task) = data.get_task(task_id) {
            if task.status == crate::models::task::TaskStatus::Failed {
                data.update_task(task_id, |t| {
                    t.status = crate::models::task::TaskStatus::Pending;
                    t.error = None;
                    t.progress = 0.0;
                });
                retried_count += 1;
            }
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "message": format!("已重新排队 {} 个失败任务", retried_count),
        "retried_count": retried_count
    }))
}

/// GET /api/v1/groups/{group_id}/download - Download all completed audio files as ZIP
pub async fn download_group_audio(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    let group = match data.get_group(&group_id) {
        Some(g) => g,
        None => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "分组不存在",
                "group_id": group_id
            }))
        }
    };

    // Collect completed tasks with audio data
    let completed_tasks: Vec<_> = group
        .task_ids
        .iter()
        .filter_map(|id| data.get_task(id))
        .filter(|t| t.status == crate::models::task::TaskStatus::Completed && t.audio_data.is_some())
        .collect();

    if completed_tasks.is_empty() {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "没有可下载的音频文件",
            "group_id": group_id
        }));
    }

    // Create ZIP archive in memory
    let mut zip_buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut zip_buffer);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (index, task) in completed_tasks.iter().enumerate() {
            let filename = if let Some(title) = &task.custom_title {
                format!("{:03}_{}.wav", index + 1, sanitize_filename(title))
            } else {
                format!("{:03}_task.wav", index + 1)
            };

            if let Some(audio_data) = &task.audio_data {
                zip.start_file(&filename, options).ok();
                zip.write_all(audio_data).ok();
            }
        }

        zip.finish().ok();
    }

    let zip_bytes = zip_buffer.into_inner();
    let group_name = sanitize_filename(&group.name);

    HttpResponse::Ok()
        .content_type("application/zip")
        .insert_header(("Content-Disposition", format!("attachment; filename=\"{}.zip\"", group_name)))
        .body(zip_bytes)
}

/// Sanitize filename by removing invalid characters
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}
