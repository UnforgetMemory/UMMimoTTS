use crate::models::response::{ErrorResponse, TaskResponse};
use crate::state::app_state::AppState;
use actix_web::{web, HttpResponse, Responder};

pub async fn list_tasks(data: web::Data<AppState>) -> impl Responder {
    let tasks = data.list_tasks();

    let task_responses: Vec<TaskResponse> = tasks
        .into_iter()
        .map(|task| TaskResponse {
            id: task.id.clone(),
            status: task.status.clone(),
            model: task.model.clone(),
            voice: task.voice.clone(),
            text: task.text.chars().take(50).collect(),
            created_at: task.created_at.to_rfc3339(),
            completed_at: task.completed_at.map(|t| t.to_rfc3339()),
            error: task.error.clone(),
            progress: task.progress,
            token_count: task.token_count,
            char_count: task.char_count,
            elapsed_secs: task.elapsed_seconds(),
            has_audio: task.audio_data.is_some(),
        })
        .collect();

    HttpResponse::Ok().json(task_responses)
}

pub async fn get_task(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let task_id = path.into_inner();

    match data.get_task(&task_id) {
        Some(task) => {
            let response = TaskResponse {
                id: task.id.clone(),
                status: task.status.clone(),
                model: task.model.clone(),
                voice: task.voice.clone(),
                text: task.text.clone(),
                created_at: task.created_at.to_rfc3339(),
                completed_at: task.completed_at.map(|t| t.to_rfc3339()),
                error: task.error.clone(),
                progress: task.progress,
                token_count: task.token_count,
                char_count: task.char_count,
                elapsed_secs: task.elapsed_seconds(),
                has_audio: task.audio_data.is_some(),
            };
            HttpResponse::Ok().json(response)
        }
        None => HttpResponse::NotFound().json(ErrorResponse {
            error: "任务不存在".to_string(),
            message: format!("任务 ID: {}", task_id),
            code: Some("TASK_NOT_FOUND".to_string()),
        }),
    }
}

pub async fn delete_task(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let task_id = path.into_inner();

    if data.remove_task(&task_id) {
        HttpResponse::Ok().json(serde_json::json!({
            "message": "任务已删除",
            "task_id": task_id
        }))
    } else {
        HttpResponse::NotFound().json(ErrorResponse {
            error: "任务不存在".to_string(),
            message: format!("任务 ID: {}", task_id),
            code: Some("TASK_NOT_FOUND".to_string()),
        })
    }
}

pub async fn get_audio(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let task_id = path.into_inner();

    match data.get_task(&task_id) {
        Some(task) => {
            if let Some(audio_data) = task.audio_data {
                HttpResponse::Ok()
                    .content_type("audio/wav")
                    .insert_header((
                        "Content-Disposition",
                        format!("attachment; filename=\"tts_{}.wav\"", task_id),
                    ))
                    .body(audio_data)
            } else {
                HttpResponse::NotFound().json(ErrorResponse {
                    error: "音频不可用".to_string(),
                    message: "任务尚未完成或合成失败".to_string(),
                    code: Some("AUDIO_NOT_AVAILABLE".to_string()),
                })
            }
        }
        None => HttpResponse::NotFound().json(ErrorResponse {
            error: "任务不存在".to_string(),
            message: format!("任务 ID: {}", task_id),
            code: Some("TASK_NOT_FOUND".to_string()),
        }),
    }
}
