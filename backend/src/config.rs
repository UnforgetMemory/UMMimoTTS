use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub mimo_api_key: String,
    pub server_port: u16,
    pub allowed_origins: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub task_cleanup_hours: u64,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        let mimo_api_key = std::env::var("MIMO_API_KEY").unwrap_or_default();
        let server_port = std::env::var("SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(30231);

        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["http://localhost:5173".to_string()]);

        let max_concurrent_tasks = std::env::var("MAX_CONCURRENT_TASKS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(5);

        let task_cleanup_hours = std::env::var("TASK_CLEANUP_HOURS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(24);

        Self {
            mimo_api_key,
            server_port,
            allowed_origins,
            max_concurrent_tasks,
            task_cleanup_hours,
        }
    }
}

pub type AppConfig = Arc<Config>;
