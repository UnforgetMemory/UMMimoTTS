use crate::config::Config;
use crate::state::app_state::AppState;
use actix_web::web;
use std::time::Duration;

/// Spawn a background task that periodically removes old audio files and completed/failed tasks
pub fn spawn_cleanup_task(data: web::Data<AppState>, config: Config) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // every hour
        loop {
            interval.tick().await;
            let cleanup_hours = config.task_cleanup_hours;
            if cleanup_hours == 0 {
                continue; // disabled
            }
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(cleanup_hours as i64);

            // Collect tasks to clean (completed/failed before cutoff)
            let tasks_to_clean: Vec<(String, Option<String>)> = {
                let tasks = data.tasks.read();
                tasks
                    .values()
                    .filter(|t| {
                        if let Some(completed) = t.completed_at {
                            completed < cutoff
                        } else {
                            false
                        }
                    })
                    .map(|t| (t.id.clone(), t.audio_path.clone()))
                    .collect()
            };

            for (task_id, audio_path) in &tasks_to_clean {
                // Remove audio file from disk
                if let Some(path) = audio_path {
                    let _ = std::fs::remove_file(path);
                }
                // Remove from memory
                data.tasks.write().remove(task_id);
            }

            if !tasks_to_clean.is_empty() {
                tracing::info!(
                    "Cleanup: removed {} old tasks (cutoff={})",
                    tasks_to_clean.len(),
                    cutoff
                );
            }
        }
    });
}
