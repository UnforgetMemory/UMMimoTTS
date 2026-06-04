//! API route handlers.

pub mod batches;
pub mod tasks;
pub mod groups;
pub mod sse;
pub mod config;
pub mod providers;

use crate::infra::persistence::provider_repo::ProviderRepo;
use crate::service::batch_service::BatchService;
use crate::service::group_service::GroupService;
use crate::service::task_service::TaskService;
use crate::infra::sse_bus::SseBus;
use actix_web::{web, HttpResponse};
use std::sync::Arc;

/// Shared application state injected into all route handlers via Actix `web::Data`.
#[derive(Clone)]
pub struct AppState {
    pub batch_service: Arc<BatchService>,
    pub task_service: Arc<TaskService>,
    pub group_service: Arc<GroupService>,
    pub provider_repo: Arc<dyn ProviderRepo>,
    pub sse_bus: Arc<SseBus>,
}

/// Backend version endpoint
async fn version() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": env!("CARGO_PKG_NAME"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
    }))
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(web::resource("/api/version").route(web::get().to(version)))
        .configure(batches::configure)
        .configure(tasks::configure)
        .configure(groups::configure)
        .configure(sse::configure)
        .configure(config::configure)
        .configure(providers::configure);
}
