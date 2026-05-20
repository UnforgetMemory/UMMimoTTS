use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "synthesizing")]
    Synthesizing,
    #[serde(rename = "streaming")]
    Streaming,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "等待中"),
            TaskStatus::Queued => write!(f, "排队中"),
            TaskStatus::Synthesizing => write!(f, "合成中"),
            TaskStatus::Streaming => write!(f, "流式加载"),
            TaskStatus::Completed => write!(f, "已完成"),
            TaskStatus::Failed => write!(f, "失败"),
            TaskStatus::Cancelled => write!(f, "已取消"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsTask {
    pub id: String,
    pub status: TaskStatus,
    pub model: String,
    pub voice: Option<String>,
    pub text: String,
    pub context: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing, skip_deserializing)]
    pub audio_data: Option<Vec<u8>>,
    pub error: Option<String>,
    pub progress: f32,
    pub token_count: usize,
    pub char_count: usize,
    pub audio_duration_secs: Option<f32>,
}

impl TtsTask {
    pub fn new(
        model: String,
        voice: Option<String>,
        text: String,
        context: Option<String>,
    ) -> Self {
        let char_count = text.chars().count();

        Self {
            id: Uuid::new_v4().to_string(),
            status: TaskStatus::Pending,
            model,
            voice,
            text,
            context,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            audio_data: None,
            error: None,
            progress: 0.0,
            token_count: 0,
            char_count,
            audio_duration_secs: None,
        }
    }

    pub fn update_status(&mut self, status: TaskStatus) {
        match status {
            TaskStatus::Synthesizing | TaskStatus::Streaming => {
                if self.started_at.is_none() {
                    self.started_at = Some(Utc::now());
                }
            }
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                self.completed_at = Some(Utc::now());
            }
            _ => {}
        }
        self.status = status;
    }

    pub fn elapsed_seconds(&self) -> Option<f64> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some((end - start).num_milliseconds() as f64 / 1000.0),
            (Some(start), None) => Some((Utc::now() - start).num_milliseconds() as f64 / 1000.0),
            _ => None,
        }
    }
}
