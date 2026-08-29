//! UM-MimoTTS v4 runtime engine.
//!
//! - `error`   — error taxonomy (ADR-013)
//! - `storage` — SQLite schema v4 + repos (SQL-pushed pagination)
//! - `throttle`— ADR-012: token buckets + AIMD gate + circuit breaker
//! - `bus`     — per-channel event fan-out
//! - `mimo`    — MimoClient v2 (official contract, streaming pcm16)
//! - `engine`  — orchestration façade (queue, workers, merge, recovery, import)

pub mod bus;
pub mod engine;
pub mod error;
pub mod mimo;
pub mod storage;
pub mod throttle;

pub use engine::{apply_env_overrides, Engine, EngineConfig, ImportResult, ProviderInfo};
pub use error::EngineError;
