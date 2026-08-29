//! /api/v3/tasks — create/list/detail/retry/cancel/delete/audio/download.

use actix_files::NamedFile;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

use mimotts_core::domain::CreateTaskInput;

use crate::auth::engine_error;
use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/tasks")
            .route("", web::get().to(list))
            .route("", web::post().to(create))
            .route("/{id}", web::get().to(get))
            .route("/{id}", web::delete().to(delete))
            .route("/{id}/retry", web::post().to(retry))
            .route("/{id}/cancel", web::post().to(cancel))
            .route("/{id}/audio", web::get().to(audio))
            .route("/{id}/download", web::get().to(download)),
    );
}

#[derive(Deserialize)]
struct ListQuery {
    page: Option<i64>,
    page_size: Option<i64>,
    status: Option<String>,
    session_id: Option<String>,
    search: Option<String>,
}

async fn list(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    q: web::Query<ListQuery>,
) -> HttpResponse {
    let page = q.page.unwrap_or(0).max(0);
    let size = q.page_size.unwrap_or(50).clamp(1, 500);
    match state.engine.list_tasks(
        page,
        size,
        q.status.as_deref(),
        q.session_id.as_deref(),
        q.search.as_deref(),
    ) {
        Ok((rows, total)) => HttpResponse::Ok().json(serde_json::json!({
            "data": rows, "total": total, "page": page, "page_size": size,
        })),
        Err(e) => engine_error(e),
    }
}

#[derive(Deserialize)]
struct CreateBody {
    title: String,
    content: String,
    voice: String,
    #[serde(default = "default_model")]
    model: String,
    style: Option<String>,
    provider_id: Option<String>,
    session_id: Option<String>,
    #[serde(default)]
    priority: i64,
}

fn default_model() -> String {
    "mimo-v2.5-tts".into()
}

async fn create(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    body: web::Json<CreateBody>,
) -> HttpResponse {
    let input = CreateTaskInput {
        session_id: body
            .session_id
            .as_deref()
            .and_then(|s| mimotts_core::domain::Id::from_str(s).ok()),
        title: body.title.clone(),
        content: body.content.clone(),
        voice: body.voice.clone(),
        model: body.model.clone(),
        style: body.style.clone(),
        priority: body.priority,
        provider_id: body.provider_id.clone(),
    };
    match state.engine.submit_task(input) {
        Ok(task) => HttpResponse::Created().json(task),
        Err(e) => engine_error(e),
    }
}

async fn get(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.task(&path) {
        Ok(Some((task, chunks))) => {
            let has_audio = state
                .engine
                .task_audio_path(&path)
                .ok()
                .flatten()
                .is_some();
            let mut json = serde_json::to_value(&task).unwrap_or_default();
            if let serde_json::Value::Object(ref mut map) = json {
                map.insert("chunks".into(), serde_json::json!(chunks));
                map.insert("has_audio".into(), serde_json::json!(has_audio));
            }
            HttpResponse::Ok().json(json)
        }
        Ok(None) => engine_error(mimotts_engine::EngineError::NotFound(format!(
            "task {}",
            path
        ))),
        Err(e) => engine_error(e),
    }
}

async fn delete(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.delete_task(&path) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => engine_error(e),
    }
}

async fn retry(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.retry_task(&path) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => engine_error(e),
    }
}

async fn cancel(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.cancel_task(&path) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => engine_error(e),
    }
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn audio(
    req: actix_web::HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    q: web::Query<TokenQuery>,
) -> HttpResponse {
    let scope = format!("audio:{}", path);
    if !crate::auth::scoped_or_bearer_ok(&req, &state.engine, q.token.as_deref(), &scope) {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "missing or invalid token", "code": "UNAUTHORIZED",
        }));
    }
    match state.engine.task_audio_path(&path) {
        Ok(Some(file)) => match NamedFile::open_async(&file).await {
            // NamedFile handles Range/206 streaming itself.
            Ok(nf) => nf.into_response(&req),
            Err(e) => engine_error(mimotts_engine::EngineError::NotFound(format!(
                "audio file: {e}"
            ))),
        },
        Ok(None) => engine_error(mimotts_engine::EngineError::NotFound(
            "task has no audio output".into(),
        )),
        Err(e) => engine_error(e),
    }
}

async fn download(
    req: actix_web::HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    q: web::Query<TokenQuery>,
) -> HttpResponse {
    let scope = format!("audio:{}", path);
    if !crate::auth::scoped_or_bearer_ok(&req, &state.engine, q.token.as_deref(), &scope) {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "missing or invalid token", "code": "UNAUTHORIZED",
        }));
    }
    match state.engine.task_audio_path(&path) {
        Ok(Some(file)) => match NamedFile::open_async(&file).await {
            Ok(nf) => nf
                .set_content_disposition(actix_web::http::header::ContentDisposition {
                    disposition: actix_web::http::header::DispositionType::Attachment,
                    parameters: vec![actix_web::http::header::DispositionParam::Filename(
                        format!("tts_{}.wav", path),
                    )],
                })
                .into_response(&req),
            Err(e) => engine_error(mimotts_engine::EngineError::NotFound(format!(
                "audio file: {e}"
            ))),
        },
        Ok(None) => engine_error(mimotts_engine::EngineError::NotFound(
            "task has no audio output".into(),
        )),
        Err(e) => engine_error(e),
    }
}
