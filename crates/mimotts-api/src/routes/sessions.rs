//! /api/v3/sessions — CRUD, cancel, export (zip).

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::engine_error;
use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/sessions")
            .route("", web::get().to(list))
            .route("", web::post().to(create))
            .route("/{id}", web::get().to(get))
            .route("/{id}", web::delete().to(delete))
            .route("/{id}/cancel", web::post().to(cancel))
            .route("/{id}/export", web::get().to(export)),
    );
}

#[derive(Deserialize)]
struct ListQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn list(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    q: web::Query<ListQuery>,
) -> HttpResponse {
    let page = q.page.unwrap_or(0).max(0);
    let size = q.page_size.unwrap_or(50).clamp(1, 500);
    match state.engine.list_sessions(page, size) {
        Ok((rows, total)) => HttpResponse::Ok().json(serde_json::json!({
            "data": rows, "total": total, "page": page, "page_size": size,
        })),
        Err(e) => engine_error(e),
    }
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
}

async fn create(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    body: web::Json<CreateBody>,
) -> HttpResponse {
    match state.engine.create_session(&body.name) {
        Ok(s) => HttpResponse::Created().json(s),
        Err(e) => engine_error(e),
    }
}

async fn get(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.session(&path) {
        Ok(Some(s)) => HttpResponse::Ok().json(s),
        Ok(None) => engine_error(mimotts_engine::EngineError::NotFound(format!(
            "session {}",
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
    match state.engine.delete_session(&path) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => engine_error(e),
    }
}

async fn cancel(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    match state.engine.cancel_session(&path) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => engine_error(e),
    }
}

async fn export(
    req: actix_web::HttpRequest,
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    path: web::Path<String>,
) -> HttpResponse {
    use actix_files::NamedFile;
    let engine = state.engine.clone();
    let id = path.into_inner();
    let outputs = match engine.session_outputs(&id) {
        Ok(o) => o,
        Err(e) => return engine_error(e),
    };
    if outputs.is_empty() {
        return engine_error(mimotts_engine::EngineError::InvalidInput(
            "session has no completed audio".into(),
        ));
    }
    // Build zip on disk with a UNIQUE temp name (concurrent exports must not
    // overwrite each other), then stream it back and delete it later.
    let tmp = std::env::temp_dir().join(format!(
        "mimotts-export-{id}-{}.zip",
        mimotts_core::domain::Id::new()
    ));
    let tmp_for_closure = tmp.clone();
    let zip_result = tokio::task::spawn_blocking(move || -> Result<(), mimotts_engine::EngineError> {
        let file = std::fs::File::create(&tmp_for_closure)?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        use std::io::Write;
        for (i, (title, path)) in outputs.iter().enumerate() {
            let data = match std::fs::read(path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let safe = sanitize_filename(&title);
            zip.start_file(format!("{i:04}_{safe}.wav"), opts)
                .map_err(|e| mimotts_engine::EngineError::Internal(format!("zip: {e}")))?;
            zip.write_all(&data)?;
        }
        zip.finish()
            .map_err(|e| mimotts_engine::EngineError::Internal(format!("zip: {e}")))?;
        Ok(())
    })
    .await;
    match zip_result {
        Ok(Ok(())) => match NamedFile::open_async(&tmp).await {
            Ok(file) => {
                // Delayed cleanup: NamedFile streams asynchronously; delete the
                // temp archive once the response has certainly been served.
                let cleanup = tmp.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                    let _ = tokio::fs::remove_file(&cleanup).await;
                });
                file.set_content_disposition(actix_web::http::header::ContentDisposition {
                    disposition: actix_web::http::header::DispositionType::Attachment,
                    parameters: vec![actix_web::http::header::DispositionParam::Filename(
                        format!("session-{id}.zip"),
                    )],
                })
                .into_response(&req)
            }
            Err(e) => engine_error(mimotts_engine::EngineError::Internal(e.to_string())),
        },
        Ok(Err(e)) => engine_error(e),
        Err(e) => engine_error(mimotts_engine::EngineError::Internal(format!(
            "zip join: {e}"
        ))),
    }
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // cap length (zip entry name limits) and strip leading/trailing dots
    cleaned.trim_matches([' ', '.']).chars().take(80).collect()
}
