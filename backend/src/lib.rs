//! UMMimoTTS v3 — Layered DDD architecture with SQLite persistence.
//!
//! This library crate exposes all public types for integration tests.
//! The binary (`main.rs`) is a thin wrapper that wires everything together.

#![allow(dead_code)]

pub mod constants;
pub mod shared;
pub mod domain;
pub mod infra;
pub mod service;
pub mod routes;

// Re-export key types for convenience.
pub use routes::AppState;
