//! Engine error taxonomy (ADR-013).
//!
//! Mirrors the MiMo official error semantics so retry/breaker logic can branch
//! without string sniffing:
//! - 421 content moderation → `ContentBlocked` (never retried)
//! - 400 context/length overflow → `ContextOverflow` (re-chunk at ×0.8)
//! - 429 → `RateLimited`; 5xx → `ServerOverload` (ADR-012 handles both)

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("rate limited")]
    RateLimited,
    #[error("server overload: {0}")]
    ServerOverload(String),
    #[error("content blocked by moderation")]
    ContentBlocked,
    #[error("context overflow: {0}")]
    ContextOverflow(String),
    #[error("no configured provider")]
    NoProvider,
    #[error("internal: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for EngineError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(format!("db: {e}"))
    }
}
impl From<r2d2::Error> for EngineError {
    fn from(e: r2d2::Error) -> Self {
        Self::Internal(format!("pool: {e}"))
    }
}
impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        Self::Internal(format!("io: {e}"))
    }
}
impl From<reqwest::Error> for EngineError {
    fn from(e: reqwest::Error) -> Self {
        Self::Internal(format!("http client: {e}"))
    }
}
impl From<serde_json::Error> for EngineError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(format!("json: {e}"))
    }
}

impl EngineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::Conflict(_) => "CONFLICT",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::RateLimited => "RATE_LIMITED",
            Self::ServerOverload(_) => "SERVER_OVERLOAD",
            Self::ContentBlocked => "CONTENT_BLOCKED",
            Self::ContextOverflow(_) => "CONTEXT_OVERFLOW",
            Self::NoProvider => "NO_PROVIDER",
            Self::Internal(_) => "INTERNAL",
        }
    }
    /// Retryable by the engine's backoff loop?
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::ServerOverload(_))
    }
}
