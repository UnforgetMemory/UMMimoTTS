use crate::db::{self, SqlitePool};
use crate::models::batch::BatchGroup;
use crate::models::response::StatsSummary;
use crate::models::task::{TaskStatus, TtsTask};
use crate::services::batch_import::BatchImportManager;
use crate::services::rate_limiter::GlobalRateLimiter;
use crate::services::stats_cache::StatsCache;
use flume::{Receiver, Sender};
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum TaskEvent {
    StatusChanged { task_id: String, status: TaskStatus, progress: f32 },
    Completed { task_id: String },
    Failed { task_id: String, error: String },
}

pub struct AppState {
    pub db_pool: SqlitePool,
    /// In-memory cache for fast SSE/audio access (also the working set for most operations).
    /// Persisted to SQLite on every write.
    pub tasks: RwLock<HashMap<String, TtsTask>>,
    pub event_senders: RwLock<HashMap<String, Vec<Sender<TaskEvent>>>>,
    pub rate_limiter: GlobalRateLimiter,
    /// In-memory cache for groups
    pub groups: RwLock<HashMap<String, BatchGroup>>,
    pub output_dir: String,
    pub stats_cache: StatsCache,
    pub batch_imports: BatchImportManager,
}

impl AppState {
    pub fn new(output_dir: String) -> Self {
        // Use default SQLite path in the output directory
        let db_path = format!("{}/mimo_tts.db", output_dir);
        let pool = db::init_db_pool(&db_path);
        Self::new_with_pool(pool, output_dir)
    }

    pub fn new_with_pool(db_pool: SqlitePool, output_dir: String) -> Self {
        // Load existing data from SQLite into in-memory cache
        let tasks = db::load_all_tasks(&db_pool);
        let groups = db::load_all_groups(&db_pool);

        let task_map: HashMap<String, TtsTask> = tasks.into_iter().map(|t| (t.id.clone(), t)).collect();
        let group_map: HashMap<String, BatchGroup> = groups.into_iter().map(|g| (g.id.clone(), g)).collect();

        let stats_cache = StatsCache::new(db_pool.clone());

        tracing::info!(
            "AppState initialized with {} tasks and {} groups from SQLite",
            task_map.len(),
            group_map.len()
        );

        Self {
            batch_imports: BatchImportManager::new(db_pool.clone()),
            db_pool,
            tasks: RwLock::new(task_map),
            event_senders: RwLock::new(HashMap::new()),
            rate_limiter: GlobalRateLimiter::new(90, 5_000_000),
            groups: RwLock::new(group_map),
            output_dir,
            stats_cache,
        }
    }

    // -----------------------------------------------------------------------
    // Task CRUD (cache + SQLite persistence)
    // -----------------------------------------------------------------------

    pub fn add_task(&self, task: TtsTask) {
        let id = task.id.clone();
        // Persist to SQLite first
        db::insert_task(&self.db_pool, &task);
        // Update in-memory cache
        self.tasks.write().insert(id.clone(), task);
        self.stats_cache.invalidate();
        tracing::info!("Task {} created", id);
    }

    pub fn get_task(&self, task_id: &str) -> Option<TtsTask> {
        let cache = self.tasks.read();
        if let Some(task) = cache.get(task_id) {
            return Some(task.clone());
        }
        // Cache miss: try DB (for data that may have been cleaned from cache)
        drop(cache);
        if let Some(task) = db::get_task_from_db(&self.db_pool, task_id) {
            // Update cache for future lookups
            self.tasks.write().insert(task_id.to_string(), task.clone());
            return Some(task);
        }
        None
    }

    pub fn update_task<F>(&self, task_id: &str, update_fn: F) -> Option<TtsTask>
    where
        F: FnOnce(&mut TtsTask),
    {
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            update_fn(task);
            let updated_task = task.clone();

            // Persist to SQLite
            db::update_task(&self.db_pool, &updated_task);
            self.stats_cache.invalidate();

            // Send events based on status
            match task.status {
                TaskStatus::Completed => {
                    self.notify_event(TaskEvent::Completed {
                        task_id: task_id.to_string(),
                    });
                }
                TaskStatus::Failed => {
                    let error = task.error.clone().unwrap_or_default();
                    self.notify_event(TaskEvent::Failed {
                        task_id: task_id.to_string(),
                        error,
                    });
                }
                _ => {
                    self.notify_event(TaskEvent::StatusChanged {
                        task_id: task_id.to_string(),
                        status: task.status.clone(),
                        progress: task.progress,
                    });
                }
            }

            Some(updated_task)
        } else {
            None
        }
    }

    pub fn remove_task(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.write();
        let removed = tasks.remove(task_id).is_some();
        if removed {
            // Delete from SQLite too
            db::delete_task(&self.db_pool, task_id);
            self.stats_cache.invalidate();
            tracing::info!("Task {} removed", task_id);
        }
        removed
    }

    pub fn list_tasks(&self) -> Vec<TtsTask> {
        self.tasks.read().values().cloned().collect()
    }

    /// Paginated listing from SQLite with filtering and sorting
    pub fn list_tasks_paginated(
        &self,
        page: usize,
        per_page: usize,
        status: Option<&str>,
        search: Option<&str>,
        sort: Option<&str>,
        group_id: Option<&str>,
    ) -> (Vec<TtsTask>, usize) {
        db::list_tasks_from_db(&self.db_pool, page, per_page, status, search, sort, group_id)
    }

    /// Paginated listing of tasks in a group
    pub fn list_group_tasks_paginated(
        &self,
        group_id: &str,
        page: usize,
        per_page: usize,
    ) -> (Vec<TtsTask>, usize) {
        db::list_tasks_from_db(&self.db_pool, page, per_page, None, None, None, Some(group_id))
    }

    pub fn update_task_title(&self, task_id: &str, title: String) -> Option<TtsTask> {
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            task.custom_title = if title.is_empty() {
                None
            } else {
                Some(title)
            };
            let updated = task.clone();
            db::update_task(&self.db_pool, &updated);
            Some(updated)
        } else {
            None
        }
    }

    // -----------------------------------------------------------------------
    // SSE event subscriptions
    // -----------------------------------------------------------------------

    pub fn subscribe_events(&self, task_id: String) -> Receiver<TaskEvent> {
        let (tx, rx) = flume::bounded::<TaskEvent>(100);
        self.event_senders
            .write()
            .entry(task_id)
            .or_insert_with(Vec::new)
            .push(tx);
        rx
    }

    fn notify_event(&self, event: TaskEvent) {
        let task_id = match &event {
            TaskEvent::StatusChanged { task_id, .. } => task_id,
            TaskEvent::Completed { task_id } => task_id,
            TaskEvent::Failed { task_id, .. } => task_id,
        };

        let senders = self.event_senders.read();
        if let Some(sender_list) = senders.get(task_id) {
            for sender in sender_list {
                let _ = sender.try_send(event.clone());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Group CRUD (cache + SQLite persistence)
    // -----------------------------------------------------------------------

    pub fn add_group(&self, group: BatchGroup) {
        let id = group.id.clone();
        // Persist to SQLite first
        db::insert_group(&self.db_pool, &group);
        // Update in-memory cache
        self.groups.write().insert(id.clone(), group);
        tracing::info!("Group {} created", id);
    }

    pub fn get_group(&self, id: &str) -> Option<BatchGroup> {
        let cache = self.groups.read();
        if let Some(group) = cache.get(id) {
            return Some(group.clone());
        }
        // Cache miss: try DB
        drop(cache);
        if let Some(group) = db::get_group_from_db(&self.db_pool, id) {
            self.groups.write().insert(id.to_string(), group.clone());
            return Some(group);
        }
        None
    }

    pub fn update_group(&self, id: &str, updater: impl FnOnce(&mut BatchGroup)) -> bool {
        let mut groups = self.groups.write();
        if let Some(group) = groups.get_mut(id) {
            updater(group);
            // Persist to SQLite
            db::update_group(&self.db_pool, group);
            true
        } else {
            false
        }
    }

    pub fn remove_group(&self, id: &str) -> Option<BatchGroup> {
        let mut groups = self.groups.write();
        let removed = groups.remove(id);
        if removed.is_some() {
            db::delete_group(&self.db_pool, id);
        }
        removed
    }

    pub fn list_groups(&self) -> Vec<BatchGroup> {
        self.groups.read().values().cloned().collect()
    }

    /// Paginated group listing from SQLite
    pub fn list_groups_paginated(&self, page: usize, per_page: usize) -> (Vec<BatchGroup>, usize) {
        db::list_groups_from_db(&self.db_pool, page, per_page)
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    pub fn get_stats_summary(&self) -> StatsSummary {
        self.stats_cache.get_or_refresh()
    }

    pub fn get_group_stats(&self, group_id: &str) -> StatsSummary {
        db::compute_group_stats(&self.db_pool, group_id)
    }
}
