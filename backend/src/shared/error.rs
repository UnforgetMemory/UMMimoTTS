use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::Serialize;
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Rate limited")]
    RateLimited,
    /// Server overload (HTTP 500/502/503/504) — retryable with backoff.
    #[error("Server overload: {0}")]
    ServerOverload(String),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self { Self::Internal(e.to_string()) }
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self { Self::Internal(e.to_string()) }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self { Self::Internal(e.to_string()) }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { Self::Internal(e.to_string()) }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::ServerOverload(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorResponse {
            error: self.to_string(),
            code: format!("{:?}", self),
        })
    }
}
