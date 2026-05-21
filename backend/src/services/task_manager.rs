use crate::models::task::{TaskStatus, TtsTask};
use crate::services::mimo_client::{MimoClient, MimoError, split_text_into_chunks};
use crate::services::token_counter;
use crate::state::app_state::AppState;
use actix_web::web::Data;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing;

pub struct TaskManager {
    pub state: Data<AppState>,
    pub semaphore: Arc<Semaphore>,
}

impl TaskManager {
    pub fn new(state: Data<AppState>, max_concurrent: usize) -> Self {
        Self {
            state,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn create_task(
        &self,
        model: String,
        voice: Option<String>,
        text: String,
        context: Option<String>,
        api_key: Option<String>,
    ) -> Result<TtsTask, String> {
        // 验证必要参数
        if text.trim().is_empty() {
            return Err("文本不能为空".to_string());
        }

        let voice = voice.ok_or_else(|| "音色不能为空".to_string())?;

        if model == "mimo-v2.5-tts" && voice.trim().is_empty() {
            return Err("预置音色模型必须指定音色".to_string());
        }

        // 创建任务
        let mut task = TtsTask::new(model.clone(), Some(voice), text, context);

        // 计算 token
        let token_count = token_counter::count_tokens_approx(&task.text);
        task.token_count = token_count;

        let task_id = task.id.clone();
        self.state.add_task(task);

        // 启动异步合成任务
        let state = self.state.clone();
        let semaphore = self.semaphore.clone();
        let task_id_clone = task_id.clone();

        let actual_api_key = api_key.unwrap_or_default();

        tokio::spawn(async move {
            Self::process_task(state, task_id_clone, model, actual_api_key, semaphore).await;
        });

        Ok(self.state.get_task(&task_id).unwrap())
    }

    async fn process_task(
        state: Data<AppState>,
        task_id: String,
        model: String,
        api_key: String,
        semaphore: Arc<Semaphore>,
    ) {
        // 获取信号量许可
        let permit = semaphore.acquire().await;
        if permit.is_err() {
            state.update_task(&task_id, |task| {
                task.update_status(TaskStatus::Failed);
                task.error = Some("获取执行许可失败".to_string());
            });
            return;
        }

        // 获取任务信息
        let task = match state.get_task(&task_id) {
            Some(t) => t,
            None => return,
        };

        let voice = task.voice.clone().unwrap();
        let text = task.text.clone();
        let context = task.context.clone();
        let actual_api_key = if api_key.is_empty() {
            // 使用环境变量中的 API Key
            std::env::var("MIMO_API_KEY").unwrap_or_default()
        } else {
            api_key
        };

        if actual_api_key.is_empty() {
            state.update_task(&task_id, |task| {
                task.update_status(TaskStatus::Failed);
                task.error = Some("API Key 未配置".to_string());
            });
            return;
        }

        // 更新状态为排队中
        state.update_task(&task_id, |task| {
            task.update_status(TaskStatus::Queued);
            task.progress = 0.1;
        });

        // 调用 MIMO API（支持分片合成）
        let client = MimoClient::new(actual_api_key);

        // 预计算分片信息
        let chunks = split_text_into_chunks(&text);
        let total_chunks = chunks.len();

        // 更新状态为合成中（API 调用前）
        state.update_task(&task_id, |task| {
            task.update_status(TaskStatus::Synthesizing);
            task.progress = 0.2;
            task.total_chunks = Some(total_chunks);
            task.current_chunk = Some(0);
        });

        // 使用分片合成
        let state_clone = state.clone();
        let task_id_clone = task_id.clone();

        match client
            .synthesize_chunked(
                &model,
                &text,
                &voice,
                context.as_deref(),
                move |current, total| {
                    // 更新分片进度
                    state_clone.update_task(&task_id_clone, |task| {
                        task.current_chunk = Some(current);
                        // 进度：0.2 (开始) ~ 0.6 (合成完成)，线性增长
                        task.progress = 0.2 + (current as f32 / total as f32) * 0.4;
                    });
                },
            )
            .await
        {
            Ok(audio_data) => {
                tracing::info!(
                    "Task {} completed successfully, audio size: {} bytes, chunks: {}",
                    task_id,
                    audio_data.len(),
                    total_chunks
                );

                // 音频数据接收完毕，准备完成
                state.update_task(&task_id, |task| {
                    task.audio_data = Some(audio_data);
                    task.progress = 0.7;
                    task.current_chunk = Some(total_chunks);
                });

                // 短暂延迟让 progress 过渡可见
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                state.update_task(&task_id, |task| {
                    task.update_status(TaskStatus::Streaming);
                    task.progress = 0.9;
                });

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                state.update_task(&task_id, |task| {
                    task.update_status(TaskStatus::Completed);
                    task.progress = 1.0;
                });
            }
            Err(e) => {
                tracing::error!("Task {} failed: {}", task_id, e);

                let error_msg = match e {
                    MimoError::InvalidApiKey => "API Key 无效，请检查配置".to_string(),
                    MimoError::RateLimitExceeded => "请求频率超限，请稍后重试".to_string(),
                    MimoError::NoAudioData => "API 未返回音频数据".to_string(),
                    MimoError::RetryExhausted(msg) => msg,
                    MimoError::ApiError { code, message } => {
                        format!("API 错误 ({}): {}", code, message)
                    }
                    MimoError::HttpError(e) => format!("网络错误: {}", e),
                };

                state.update_task(&task_id, |task| {
                    task.update_status(TaskStatus::Failed);
                    task.error = Some(error_msg);
                    task.progress = 0.0;
                });
            }
        }

        // 释放信号量许可
        drop(permit);
    }
}
