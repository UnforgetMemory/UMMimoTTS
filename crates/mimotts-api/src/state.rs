//! Shared app state.

use std::sync::Arc;

use mimotts_engine::Engine;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Engine>,
    /// Static UI directory (dev: apps/web/dist; the `embed-ui` feature
    /// compiles it into the single binary instead).
    pub ui_dist: Option<std::path::PathBuf>,
}

impl AppState {
    pub fn new(engine: Arc<Engine>, ui_dist: Option<std::path::PathBuf>) -> Self {
        Self { engine, ui_dist }
    }
}
