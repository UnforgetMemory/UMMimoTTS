use crate::models::response::{ErrorResponse, TaskResponse};
use crate::state::app_state::AppState;
use actix_web::http::header;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde::Deserialize;

pub async fn list_tasks(data: web::Data<AppState>) -> impl Responder {
    let tasks = data.list_tasks();

    let task_responses: Vec<TaskResponse> = tasks
        .into_iter()
        .map(|task| TaskResponse {
            id: task.id.clone(),
            custom_title: task.custom_title.clone(),
            status: task.status.clone(),
            model: task.model.clone(),
            voice: task.voice.clone(),
            text: task.text.clone(),
            context: task.context.clone(),
            created_at: task.created_at.to_rfc3339(),
            completed_at: task.completed_at.map(|t| t.to_rfc3339()),
            error: task.error.clone(),
            progress: task.progress,
            token_count: task.token_count,
            char_count: task.char_count,
            elapsed_secs: task.elapsed_seconds(),
            has_audio: task.audio_data.is_some(),
            total_chunks: task.total_chunks,
            current_chunk: task.current_chunk,
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
                custom_title: task.custom_title.clone(),
                status: task.status.clone(),
                model: task.model.clone(),
                voice: task.voice.clone(),
                text: task.text.clone(),
                context: task.context.clone(),
                created_at: task.created_at.to_rfc3339(),
                completed_at: task.completed_at.map(|t| t.to_rfc3339()),
                error: task.error.clone(),
                progress: task.progress,
                token_count: task.token_count,
                char_count: task.char_count,
                elapsed_secs: task.elapsed_seconds(),
                has_audio: task.audio_data.is_some(),
                total_chunks: task.total_chunks,
                current_chunk: task.current_chunk,
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

pub async fn get_audio(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let task_id = path.into_inner();

    match data.get_task(&task_id) {
        Some(task) => {
            if let Some(audio_data) = task.audio_data {
                let audio_len = audio_data.len() as u64;

                // Parse Range header: "bytes=start-end", "bytes=start-", or "bytes=-suffix"
                let range_header = req
                    .headers()
                    .get(header::RANGE)
                    .and_then(|v| v.to_str().ok());

                if let Some(range_str) = range_header {
                    if let Some(range_val) = range_str.strip_prefix("bytes=") {
                        if let Some(range) = parse_byte_range(range_val, audio_len) {
                            let (start, end) = range;
                            let body = &audio_data[start as usize..=end as usize];
                            let content_range = format!("bytes {}-{}/{}", start, end, audio_len);

                            return HttpResponse::PartialContent()
                                .content_type("audio/wav")
                                .insert_header(("Accept-Ranges", "bytes"))
                                .insert_header(("Content-Range", content_range.as_str()))
                                .insert_header(("Content-Length", body.len().to_string()))
                                .insert_header((
                                    "Content-Disposition",
                                    format!("inline; filename=\"tts_{}.wav\"", task_id),
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
                        format!("inline; filename=\"tts_{}.wav\"", task_id),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::task::TtsTask;
    use crate::state::app_state::AppState;
    use actix_web::test as actix_test;
    use actix_web::App;

    fn setup_app_with_state() -> (web::Data<AppState>, String) {
        let state = AppState::new();
        let task_id = "test-audio-001".to_string();
        let mut task = TtsTask::new(
            "test-model".into(),
            Some("test-voice".into()),
            "hello".into(),
            None,
        );
        task.id = task_id.clone();
        task.status = crate::models::task::TaskStatus::Completed;
        task.audio_data = Some(vec![0u8; 1000]);
        state.add_task(task);
        (web::Data::new(state), task_id)
    }

    #[actix_web::test]
    async fn test_get_audio_no_range() {
        let (data, id) = setup_app_with_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(data.clone())
                .route("/api/v1/tasks/{task_id}/audio", web::get().to(get_audio)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri(&format!("/api/v1/tasks/{}/audio", id))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;

        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()
                .get("Accept-Ranges")
                .unwrap()
                .to_str()
                .unwrap(),
            "bytes"
        );
        assert_eq!(
            resp.headers().get("Content-Type").unwrap().to_str().unwrap(),
            "audio/wav"
        );
    }

    #[actix_web::test]
    async fn test_get_audio_with_range() {
        let (data, id) = setup_app_with_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(data.clone())
                .route("/api/v1/tasks/{task_id}/audio", web::get().to(get_audio)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri(&format!("/api/v1/tasks/{}/audio", id))
            .insert_header(("Range", "bytes=0-99"))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;

        assert_eq!(resp.status(), 206);
        assert_eq!(
            resp.headers()
                .get("Content-Range")
                .unwrap()
                .to_str()
                .unwrap(),
            "bytes 0-99/1000"
        );
        assert_eq!(
            resp.headers()
                .get("Content-Length")
                .unwrap()
                .to_str()
                .unwrap(),
            "100"
        );
        let body = actix_test::read_body(resp).await;
        assert_eq!(body.len(), 100);
    }

    #[actix_web::test]
    async fn test_get_audio_range_middle() {
        let (data, id) = setup_app_with_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(data.clone())
                .route("/api/v1/tasks/{task_id}/audio", web::get().to(get_audio)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri(&format!("/api/v1/tasks/{}/audio", id))
            .insert_header(("Range", "bytes=200-499"))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;

        assert_eq!(resp.status(), 206);
        assert_eq!(
            resp.headers()
                .get("Content-Range")
                .unwrap()
                .to_str()
                .unwrap(),
            "bytes 200-499/1000"
        );
        assert_eq!(actix_test::read_body(resp).await.len(), 300);
    }

    #[actix_web::test]
    async fn test_get_audio_nonexistent() {
        let state = AppState::new();
        let data = web::Data::new(state);
        let app = actix_test::init_service(
            App::new()
                .app_data(data.clone())
                .route("/api/v1/tasks/{task_id}/audio", web::get().to(get_audio)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/api/v1/tasks/nonexistent/audio")
            .insert_header(("Range", "bytes=0-99"))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;

        assert_eq!(resp.status(), 404);
    }

    #[test]
    fn test_parse_byte_range_normal() {
        // bytes=0-99 for a 1000-byte file
        assert_eq!(parse_byte_range("0-99", 1000), Some((0, 99)));
    }

    #[test]
    fn test_parse_byte_range_open_ended() {
        // bytes=500- for a 1000-byte file
        assert_eq!(parse_byte_range("500-", 1000), Some((500, 999)));
    }

    #[test]
    fn test_parse_byte_range_suffix() {
        // bytes=-500 for a 1000-byte file → last 500 bytes
        assert_eq!(parse_byte_range("-500", 1000), Some((500, 999)));
    }

    #[test]
    fn test_parse_byte_range_exact() {
        assert_eq!(parse_byte_range("0-0", 1), Some((0, 0)));
    }

    #[test]
    fn test_parse_byte_range_start_beyond_total() {
        assert_eq!(parse_byte_range("1000-", 1000), None);
    }

    #[test]
    fn test_parse_byte_range_empty_file() {
        assert_eq!(parse_byte_range("0-0", 0), None);
    }

    #[test]
    fn test_parse_byte_range_suffix_larger_than_file() {
        // Suffix larger than file → return start=0
        assert_eq!(parse_byte_range("-9999", 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_byte_range_suffix_zero() {
        assert_eq!(parse_byte_range("-0", 1000), None);
    }

    #[test]
    fn test_parse_byte_range_invalid() {
        assert_eq!(parse_byte_range("abc-", 1000), None);
    }

    #[test]
    fn test_parse_byte_range_start_after_end() {
        assert_eq!(parse_byte_range("500-100", 1000), None);
    }
}

/// Parse a byte range value (without "bytes=" prefix) and return (start, end) inclusive.
/// Supports three formats:
///   "start-end"     → bytes start through end
///   "start-"        → bytes start to end of file
///   "-suffix"       → last suffix bytes
fn parse_byte_range(range_val: &str, total: u64) -> Option<(u64, u64)> {
    if total == 0 {
        return None;
    }

    if let Some((start_str, end_str)) = range_val.split_once('-') {
        if start_str.is_empty() {
            // Suffix range: "-500" → last 500 bytes
            let suffix: u64 = end_str.parse().ok()?;
            if suffix == 0 {
                return None;
            }
            let start = total.saturating_sub(suffix.min(total));
            let end = total - 1;
            Some((start, end))
        } else {
            let start: u64 = start_str.parse().ok()?;
            if start >= total {
                return None;
            }
            let end = if end_str.is_empty() {
                total - 1
            } else {
                end_str.parse::<u64>().ok()?.min(total - 1)
            };
            if start > end {
                return None;
            }
            Some((start, end))
        }
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateTitleRequest {
    pub title: Option<String>,
}

pub async fn update_task_title(
    path: web::Path<String>,
    body: web::Json<UpdateTitleRequest>,
    data: web::Data<AppState>,
) -> impl Responder {
    let task_id = path.into_inner();

    let new_title = body.title.clone().unwrap_or_default();

    let updated = data.update_task(&task_id, |task| {
        task.custom_title = Some(new_title.clone());
    });

    match updated {
        Some(task) => HttpResponse::Ok().json(TaskResponse {
            id: task.id.clone(),
            custom_title: task.custom_title.clone(),
            status: task.status.clone(),
            model: task.model.clone(),
            voice: task.voice.clone(),
            text: task.text.clone(),
            context: task.context.clone(),
            created_at: task.created_at.to_rfc3339(),
            completed_at: task.completed_at.map(|t| t.to_rfc3339()),
            error: task.error.clone(),
            progress: task.progress,
            token_count: task.token_count,
            char_count: task.char_count,
            elapsed_secs: task.elapsed_seconds(),
            has_audio: task.audio_data.is_some(),
            total_chunks: task.total_chunks,
            current_chunk: task.current_chunk,
        }),
        None => HttpResponse::NotFound().json(ErrorResponse {
            error: "任务不存在".to_string(),
            message: format!("任务 ID: {}", task_id),
            code: Some("TASK_NOT_FOUND".to_string()),
        }),
    }
}
