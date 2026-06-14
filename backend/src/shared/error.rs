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
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
        }
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::Conflict(_) => "CONFLICT",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::RateLimited => "RATE_LIMITED",
            Self::ServerOverload(_) => "SERVER_OVERLOAD",
        }
    }

    pub fn error_response(&self) -> ErrorResponse {
        ErrorResponse::new(self.to_string(), self.code())
    }
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
        let retry_after = match self {
            Self::RateLimited => Some(30),
            Self::ServerOverload(_) => Some(60),
            _ => None,
        };
        let mut builder = HttpResponse::build(self.status_code());
        if let Some(seconds) = retry_after {
            builder.append_header((
                actix_web::http::header::RETRY_AFTER,
                seconds.to_string(),
            ));
        }
        builder.json(AppError::error_response(self))
    }
}
