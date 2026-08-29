//! POST /api/v3/import — batch TXT upload (multipart, UTF-8/GB18030).

use actix_multipart::form::tempfile::TempFile;
use actix_multipart::form::MultipartForm;
use actix_web::{web, HttpResponse};

use crate::auth::engine_error;
use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/import", web::post().to(import));
}

#[derive(Debug, MultipartForm)]
struct ImportForm {
    #[multipart(rename = "files")]
    files: Vec<TempFile>,
    session_id: Option<actix_multipart::form::text::Text<String>>,
    session_name: Option<actix_multipart::form::text::Text<String>>,
    voice: Option<actix_multipart::form::text::Text<String>>,
    model: Option<actix_multipart::form::text::Text<String>>,
    style: Option<actix_multipart::form::text::Text<String>>,
}

const MAX_FILE_BYTES: usize = 32 * 1024 * 1024; // 32MB per txt
const MAX_FILES: usize = 500; // upload count cap (umreview)
const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024; // 256MB total per import

async fn import(
    state: web::Data<AppState>,
    _auth: crate::auth::Auth,
    MultipartForm(form): MultipartForm<ImportForm>,
) -> HttpResponse {
    let voice = form
        .voice
        .as_deref()
        .map(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(mimotts_core::catalog::DEFAULT_VOICE)
        .to_string();
    let model = form
        .model
        .as_deref()
        .map(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(mimotts_core::catalog::DEFAULT_MODEL)
        .to_string();
    let style = form
        .style
        .as_deref()
        .map(|t| t.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string());
    let session_id = form
        .session_id
        .as_deref()
        .map(|t| t.as_str().to_string());
    let session_name = form
        .session_name
        .as_deref()
        .map(|t| t.as_str().to_string());

    if form.files.len() > MAX_FILES {
        return engine_error(mimotts_engine::EngineError::InvalidInput(format!(
            "超过单次导入上限（{} 个文件）",
            MAX_FILES
        )));
    }
    let mut total_bytes = 0usize;
    let mut files = Vec::new();
    for f in &form.files {
        let name = f
            .file_name
            .clone()
            .unwrap_or_else(|| "untitled.txt".into());
        if f.size > MAX_FILE_BYTES {
            return engine_error(mimotts_engine::EngineError::InvalidInput(format!(
                "{name} 超过 32MB 上限"
            )));
        }
        total_bytes += f.size;
        if total_bytes > MAX_TOTAL_BYTES {
            return engine_error(mimotts_engine::EngineError::InvalidInput(
                "单次导入总量超过 256MB 上限".into(),
            ));
        }
        let data = match std::fs::read(f.file.path()) {
            Ok(d) => d,
            Err(e) => {
                return engine_error(mimotts_engine::EngineError::Internal(format!(
                    "read {name}: {e}"
                )))
            }
        };
        files.push((name, data));
    }
    if files.is_empty() {
        return engine_error(mimotts_engine::EngineError::InvalidInput(
            "没有收到任何文件".into(),
        ));
    }
    match state
        .engine
        .import_files(session_id, session_name, &voice, &model, style, None, files)
    {
        Ok(result) => HttpResponse::Accepted().json(result),
        Err(e) => engine_error(e),
    }
}
