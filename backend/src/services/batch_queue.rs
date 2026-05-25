use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use crate::services::rate_limiter::GlobalRateLimiter;
use crate::state::app_state::AppState;
use crate::models::batch::GroupStatus;
use crate::models::task::TaskStatus;

/// A task waiting in the batch queue
#[derive(Debug, Clone)]
pub struct QueuedTask {
    pub task_id: String,
    pub group_id: Option<String>,
    pub priority: u8,                   // 0=highest, 255=lowest
    pub token_count: usize,
    pub enqueued_at: Instant,
}

/// Queue statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueueStats {
    pub pending_count: usize,
    pub active_count: usize,
    pub total_processed: u64,
    pub total_failed: u64,
    pub rate_limiter_stats: crate::services::rate_limiter::RateLimiterStats,
}

/// Per-group failure tracking for circuit breaker
#[derive(Debug, Clone)]
struct GroupFailureTracker {
    /// IDs of groups that should be skipped (paused or circuit-broken)
    paused_groups: std::collections::HashSet<String>,
}

impl GroupFailureTracker {
    fn new() -> Self {
        Self {
            paused_groups: std::collections::HashSet::new(),
        }
    }

    fn is_paused(&self, group_id: &str) -> bool {
        self.paused_groups.contains(group_id)
    }

    fn pause_group(&mut self, group_id: &str) {
        self.paused_groups.insert(group_id.to_string());
    }

    fn resume_group(&mut self, group_id: &str) {
        self.paused_groups.remove(group_id);
    }
}

/// Priority batch queue with rate limiting and circuit breaker
#[derive(Clone)]
pub struct BatchQueue {
    inner: Arc<Mutex<BatchQueueInner>>,
    rate_limiter: GlobalRateLimiter,
    notify: Arc<Notify>,
    max_concurrent_cap: Arc<AtomicUsize>,
}

struct BatchQueueInner {
    pending: VecDeque<QueuedTask>,
    max_concurrent: usize,
    active_count: usize,
    total_processed: u64,
    total_failed: u64,
    /// Consecutive failures per group (for circuit breaker)
    group_consecutive_failures: std::collections::HashMap<String, usize>,
    /// Groups that are paused (manually or by circuit breaker)
    paused_groups: std::collections::HashSet<String>,
    /// Circuit breaker threshold: pause group after this many consecutive failures
    circuit_breaker_threshold: usize,
}

impl BatchQueue {
    pub fn new(rate_limiter: GlobalRateLimiter, max_concurrent: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BatchQueueInner {
                pending: VecDeque::new(),
                max_concurrent,
                active_count: 0,
                total_processed: 0,
                total_failed: 0,
                group_consecutive_failures: std::collections::HashMap::new(),
                paused_groups: std::collections::HashSet::new(),
                circuit_breaker_threshold: 3, // Pause after 3 consecutive failures
            })),
            rate_limiter,
            notify: Arc::new(Notify::new()),
            max_concurrent_cap: Arc::new(AtomicUsize::new(max_concurrent)),
        }
    }

    /// Add a task to the queue (sorted by priority)
    pub async fn enqueue(&self, task: QueuedTask) {
        let mut inner = self.inner.lock().await;
        
        // Insert in priority order (lower priority number = higher priority)
        let insert_pos = inner.pending.iter()
            .position(|t| t.priority > task.priority)
            .unwrap_or(inner.pending.len());
        
        inner.pending.insert(insert_pos, task);
        tracing::info!("Task enqueued, pending: {}", inner.pending.len());
        
        // Notify consumer that a new task is available
        self.notify.notify_one();
    }

    /// Try to dequeue the next task (respects concurrency limits and pause state)
    /// Skips tasks belonging to paused groups
    pub async fn try_dequeue(&self) -> Option<QueuedTask> {
        let mut inner = self.inner.lock().await;
        
        if inner.active_count >= inner.max_concurrent {
            return None;
        }
        
        // Find the first task whose group is NOT paused
        let idx = inner.pending.iter().position(|t| {
            if let Some(ref gid) = t.group_id {
                !inner.paused_groups.contains(gid)
            } else {
                true // No group_id = single task, always process
            }
        });
        
        if let Some(idx) = idx {
            let task = inner.pending.remove(idx).unwrap();
            inner.active_count += 1;
            Some(task)
        } else {
            None
        }
    }

    /// Mark a task as completed/failure and manage circuit breaker
    pub async fn complete_task(&self, task_id: &str, group_id: Option<&str>, success: bool) {
        let mut inner = self.inner.lock().await;
        inner.active_count = inner.active_count.saturating_sub(1);
        
        if success {
            inner.total_processed += 1;
            // Reset consecutive failures for this group on success
            if let Some(gid) = group_id {
                inner.group_consecutive_failures.remove(gid);
            }
        } else {
            inner.total_failed += 1;
            // Track consecutive failures per group
            if let Some(gid) = group_id {
                let count = inner.group_consecutive_failures
                    .entry(gid.to_string())
                    .or_insert(0);
                *count += 1;
                let current_count = *count;
                
                // Circuit breaker: pause group after N consecutive failures
                if current_count >= inner.circuit_breaker_threshold {
                    tracing::warn!(
                        "Circuit breaker triggered for group {} after {} consecutive failures",
                        gid, current_count
                    );
                    inner.paused_groups.insert(gid.to_string());
                }
            }
        }
        
        // Notify consumer to try dequeuing more tasks
        drop(inner);
        self.notify.notify_one();
    }

    /// Pause a group (manually or by circuit breaker)
    pub async fn pause_group(&self, group_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.paused_groups.insert(group_id.to_string());
        tracing::info!("Group {} paused in queue", group_id);
    }

    /// Resume a group and reset its failure counter
    pub async fn resume_group(&self, group_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.paused_groups.remove(group_id);
        inner.group_consecutive_failures.remove(group_id);
        tracing::info!("Group {} resumed in queue", group_id);
        drop(inner);
        self.notify.notify_one();
    }

    /// Check if a group is paused
    pub async fn is_group_paused(&self, group_id: &str) -> bool {
        let inner = self.inner.lock().await;
        inner.paused_groups.contains(group_id)
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> QueueStats {
        let inner = self.inner.lock().await;
        let rate_stats = self.rate_limiter.get_stats().await;
        
        QueueStats {
            pending_count: inner.pending.len(),
            active_count: inner.active_count,
            total_processed: inner.total_processed,
            total_failed: inner.total_failed,
            rate_limiter_stats: rate_stats,
        }
    }

    /// Get number of pending tasks
    pub async fn pending_count(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.pending.len()
    }

    /// Check if queue has capacity
    pub async fn has_capacity(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.active_count < inner.max_concurrent
    }

    /// Get the rate limiter reference
    pub fn rate_limiter(&self) -> &GlobalRateLimiter {
        &self.rate_limiter
    }

    /// Dynamically adjust max_concurrent based on rate limiter feedback
    pub async fn adjust_concurrency(&self) {
        let stats = self.rate_limiter.get_stats().await;
        let rpm_pct = stats.current_rpm as f64 / stats.max_rpm as f64;
        let tpm_pct = stats.current_tpm as f64 / stats.max_tpm as f64;
        let cap = self.max_concurrent_cap.load(Ordering::Relaxed);

        let mut inner = self.inner.lock().await;
        if rpm_pct < 0.5 && tpm_pct < 0.5 && inner.max_concurrent < cap {
            inner.max_concurrent = (inner.max_concurrent + 1).min(cap);
            tracing::info!("SmartQueue: increased max_concurrent to {}", inner.max_concurrent);
        } else if (rpm_pct > 0.85 || tpm_pct > 0.85) && inner.max_concurrent > 1 {
            inner.max_concurrent = inner.max_concurrent.saturating_sub(1).max(1);
            tracing::info!("SmartQueue: decreased max_concurrent to {} (rpm={}%, tpm={}%)",
                inner.max_concurrent, (rpm_pct*100.0) as u32, (tpm_pct*100.0) as u32);
        }
    }

    /// Start background consumer that processes batch tasks
    /// Uses Notify for reliable wake-up on task enqueue AND task completion
    pub fn start_consumer(&self, state: actix_web::web::Data<AppState>) {
        let queue = self.clone();
        let state = state.clone();
        let max_workers = {
            let inner = futures::executor::block_on(queue.inner.lock());
            inner.max_concurrent
        };

        // Clone queue for adaptation loop before moving queue into consumer
        let queue_adapt = queue.clone();

        tokio::spawn(async move {
            tracing::info!("Batch consumer started, max_concurrent={}", max_workers);
            let semaphore = Arc::new(tokio::sync::Semaphore::new(max_workers));

            loop {
                // Wait for notification (task enqueued OR task completed)
                queue.notify.notified().await;

                // Try to process all available tasks
                loop {
                    // Check if we have semaphore capacity
                    let sem = semaphore.clone();
                    let permit = match sem.try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => break, // At max concurrency
                    };

                    // Try to dequeue (skips paused groups)
                    let queued = match queue.try_dequeue().await {
                        Some(t) => t,
                        None => {
                            drop(permit);
                            break; // No more tasks (or all paused)
                        }
                    };

                    let queue_clone = queue.clone();
                    let state_clone = state.clone();

                    tokio::spawn(async move {
                        let task_id = queued.task_id.clone();
                        let group_id = queued.group_id.clone();

                        tracing::info!("Processing batch task {}", task_id);

                        // Check if group is paused before processing
                        if let Some(ref gid) = group_id {
                            if let Some(group) = state_clone.get_group(gid) {
                                if group.status == GroupStatus::Paused {
                                    tracing::info!("Skipping task {} - group {} is paused", task_id, gid);
                                    // Put task back in queue
                                    queue_clone.enqueue(QueuedTask {
                                        task_id: task_id.clone(),
                                        group_id: group_id.clone(),
                                        priority: queued.priority,
                                        token_count: queued.token_count,
                                        enqueued_at: queued.enqueued_at,
                                    }).await;
                                    queue_clone.complete_task(&task_id, group_id.as_deref(), true).await;
                                    drop(permit);
                                    return;
                                }
                            }
                        }

                        let success = Self::process_batch_task(&*state_clone, &task_id).await;
                        
                        // Get group_id from task if not already known
                        let effective_group_id = group_id.or_else(|| {
                            state_clone.get_task(&task_id)
                                .and_then(|t| t.group_id.clone())
                        });
                        
                        queue_clone.complete_task(
                            &task_id,
                            effective_group_id.as_deref(),
                            success,
                        ).await;

                        // Update group progress
                        if let Some(ref gid) = effective_group_id {
                            Self::update_group_progress(&*state_clone, gid, &queue_clone).await;
                        }

                        tracing::info!("Batch task {} done, success={}", task_id, success);
                        drop(permit);
                    });
                }
            }
        });

        // Spawn smart queue adaptation loop
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                queue_adapt.adjust_concurrency().await;
            }
        });
    }

    /// Process a single batch task (TTS synthesis)
    async fn process_batch_task(state: &AppState, task_id: &str) -> bool {
        use crate::services::mimo_client::{MimoClient, split_text_into_chunks};

        // Get task info
        let task = match state.get_task(task_id) {
            Some(t) => t,
            None => {
                tracing::error!("Batch task {} not found", task_id);
                return false;
            }
        };

        let voice = match task.voice.clone() {
            Some(v) => v,
            None => {
                state.update_task(task_id, |t| {
                    t.update_status(TaskStatus::Failed);
                    t.error = Some("音色未指定".to_string());
                });
                return false;
            }
        };

        let text = task.text.clone();
        let model = task.model.clone();
        let context = task.context.clone();

        // Get API key from task (stored during batch import)
        let api_key = match task.api_key.clone() {
            Some(k) if !k.is_empty() => k,
            _ => {
                state.update_task(task_id, |t| {
                    t.update_status(TaskStatus::Failed);
                    t.error = Some("API Key 未配置".to_string());
                });
                return false;
            }
        };

        // Update status to queued
        state.update_task(task_id, |t| {
            t.update_status(TaskStatus::Queued);
            t.progress = 0.1;
        });

        // Create MimoClient with shared rate limiter
        let client = MimoClient::new(api_key, state.rate_limiter.clone());

        // Split text into chunks
        let chunks = split_text_into_chunks(&text);
        let total_chunks = chunks.len();

        // Update status to synthesizing
        state.update_task(task_id, |t| {
            t.update_status(TaskStatus::Synthesizing);
            t.progress = 0.2;
            t.total_chunks = Some(total_chunks);
            t.current_chunk = Some(0);
        });

        // Synthesize with progress callbacks
        let state_clone = state.clone();
        let task_id_clone = task_id.to_string();

        match client
            .synthesize_chunked_with_chunks(
                chunks,
                &model,
                &voice,
                context.as_deref(),
                move |current, total| {
                    state_clone.update_task(&task_id_clone, |t| {
                        t.current_chunk = Some(current);
                        t.progress = 0.2 + (current as f32 / total as f32) * 0.6;
                    });
                },
            )
            .await
        {
            Ok(audio_data) => {
                // 保存音频到磁盘
                let output_dir = state.output_dir.clone();
                let output_path = format!("{}/{}.wav", output_dir, task_id);
                if let Some(parent) = std::path::Path::new(&output_path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(&output_path, &audio_data).is_ok() {
                    state.update_task(task_id, |t| {
                        t.audio_path = Some(output_path);
                    });
                }

                state.update_task(task_id, |t| {
                    t.audio_data = Some(audio_data);
                    t.update_status(TaskStatus::Completed);
                    t.progress = 1.0;
                    t.completed_at = Some(chrono::Utc::now());
                });
                true
            }
            Err(e) => {
                let error_msg = match &e {
                    crate::services::mimo_client::MimoError::ApiError { code, message } => {
                        format!("API 错误 ({}): {}", code, message)
                    }
                    crate::services::mimo_client::MimoError::HttpError(e) => {
                        format!("网络错误: {}", e)
                    }
                    crate::services::mimo_client::MimoError::InvalidApiKey => {
                        "API Key 无效".to_string()
                    }
                    crate::services::mimo_client::MimoError::RateLimitExceeded => {
                        "请求频率超限".to_string()
                    }
                    crate::services::mimo_client::MimoError::NoAudioData => {
                        "未返回音频数据".to_string()
                    }
                    crate::services::mimo_client::MimoError::RetryExhausted(msg) => {
                        format!("重试耗尽: {}", msg)
                    }
                };
                
                tracing::error!("Batch task {} failed: {}", task_id, error_msg);
                state.update_task(task_id, |t| {
                    t.update_status(TaskStatus::Failed);
                    t.error = Some(error_msg);
                });
                false
            }
        }
    }

    /// Update group progress and check circuit breaker state
    async fn update_group_progress(state: &AppState, group_id: &str, queue: &BatchQueue) {
        let tasks: Vec<_> = {
            let all_tasks = state.tasks.read();
            all_tasks.values()
                .filter(|t| t.group_id.as_deref() == Some(group_id))
                .cloned()
                .collect()
        };

        let _total = tasks.len();
        let completed = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
        let failed = tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
        let tokens = tasks.iter().map(|t| t.token_count).sum::<usize>();
        let is_paused = queue.is_group_paused(group_id).await;

        state.update_group(group_id, |group| {
            group.update_progress(completed, failed, tokens);
            
            // If circuit breaker paused the group, update group status
            if is_paused && group.status == GroupStatus::Processing {
                group.status = GroupStatus::Paused;
                tracing::warn!("Group {} auto-paused by circuit breaker", group_id);
            }
        });
    }
}
