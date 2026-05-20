use crate::models::task::{TaskStatus, TtsTask};
use flume::{Receiver, Sender};
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum TaskEvent {
    StatusChanged { task_id: String, progress: f32 },
    Completed { task_id: String },
    Failed { task_id: String, error: String },
}

pub struct AppState {
    pub tasks: RwLock<HashMap<String, TtsTask>>,
    pub event_senders: RwLock<HashMap<String, Vec<Sender<TaskEvent>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            event_senders: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_task(&self, task: TtsTask) {
        let id = task.id.clone();
        self.tasks.write().insert(id.clone(), task);
        tracing::info!("Task {} created", id);
    }

    pub fn get_task(&self, task_id: &str) -> Option<TtsTask> {
        self.tasks.read().get(task_id).cloned()
    }

    pub fn update_task<F>(&self, task_id: &str, update_fn: F) -> Option<TtsTask>
    where
        F: FnOnce(&mut TtsTask),
    {
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            update_fn(task);
            let updated_task = task.clone();

            // 根据任务状态发送对应的事件
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
                    // 其他状态发送 StatusChanged 事件
                    self.notify_event(TaskEvent::StatusChanged {
                        task_id: task_id.to_string(),
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
            tracing::info!("Task {} removed", task_id);
        }
        removed
    }

    pub fn list_tasks(&self) -> Vec<TtsTask> {
        self.tasks.read().values().cloned().collect()
    }

    pub fn update_task_title(&self, task_id: &str, title: String) -> Option<TtsTask> {
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            task.custom_title = if title.is_empty() {
                None
            } else {
                Some(title)
            };
            Some(task.clone())
        } else {
            None
        }
    }

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
}
