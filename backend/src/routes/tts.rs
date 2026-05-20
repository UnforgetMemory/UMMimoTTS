use crate::models::request::SynthesizeRequest;
use crate::models::response::{ErrorResponse, SynthesizeResponse};
use crate::services::task_manager::TaskManager;
use crate::state::app_state::AppState;
use actix_web::{web, HttpResponse, Responder};

pub async fn synthesize(
    web::Json(request): web::Json<SynthesizeRequest>,
    data: web::Data<AppState>,
) -> impl Responder {
    let task_manager = TaskManager::new(
        data,
        std::env::var("MAX_CONCURRENT_TASKS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(5),
    );

    match task_manager
        .create_task(
            request.model,
            request.voice,
            request.text,
            request.context,
            request.api_key,
        )
        .await
    {
        Ok(task) => HttpResponse::Ok().json(SynthesizeResponse {
            task_id: task.id,
            status: task.status,
            token_count: task.token_count,
            char_count: task.char_count,
            message: "任务已创建，正在合成中".to_string(),
        }),
        Err(e) => HttpResponse::BadRequest().json(ErrorResponse {
            error: "创建任务失败".to_string(),
            message: e,
            code: Some("TASK_CREATION_FAILED".to_string()),
        }),
    }
}
