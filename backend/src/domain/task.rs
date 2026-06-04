use crate::shared::id::Id;
use crate::shared::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,    // created, awaiting TaskQueue
    Queued,     // in TaskQueue, awaiting chunker
    Chunking,   // chunker running
    Processing, // ≥1 chunk in ChunkQueue or being processed
    Merging,    // all chunks Done, merging final audio
    MergingFailed, // merge failed, retryable
    Paused,     // paused by user or parent group
    Done,       // task complete (audio merged)
    Failed,     // all chunks Failed/Dead, unrecoverable
    Cancelled,  // cancelled by user or parent group
}

impl TaskStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::Pending, Self::Queued)
            | (Self::Queued, Self::Chunking)
            | (Self::Queued, Self::Processing)  // worker picks first chunk
            | (Self::Chunking, Self::Processing)
            | (Self::Processing, Self::Merging)
            | (Self::Merging, Self::Done)
            | (Self::Merging, Self::MergingFailed)
            | (Self::MergingFailed, Self::Merging)  // retry merge
            | (Self::MergingFailed, Self::Done)
            | (Self::Processing, Self::Failed)
            | (Self::Failed, Self::Queued)  // manual retry
            | (Self::Queued, Self::Paused)
            | (Self::Chunking, Self::Paused)
            | (Self::Processing, Self::Paused)
            | (Self::Merging, Self::Paused)
            | (Self::MergingFailed, Self::Paused)
            | (Self::Paused, Self::Queued)
            | (Self::Paused, Self::Chunking)
            | (Self::Paused, Self::Processing)
            | (Self::Pending, Self::Cancelled)
            | (Self::Queued, Self::Cancelled)
            | (Self::Chunking, Self::Cancelled)
            | (Self::Processing, Self::Cancelled)
            | (Self::Merging, Self::Cancelled)
            | (Self::MergingFailed, Self::Cancelled)
            | (Self::Paused, Self::Cancelled)
            | (Self::Failed, Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    Single,
    BatchChild,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub batch_id: Option<Id>,
    pub group_id: Option<Id>,
    pub content: String,
    pub content_ref: Option<String>,
    pub title: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub speed: f64,
    pub total_chars: i64,
    pub total_tokens: i64,
    pub total_chunks: i32,
    pub done_chunks: i32,
    pub failed_chunks: i32,
    pub output_path: Option<String>,
    pub provider_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub task_type: TaskType,
    pub batch_id: Option<Id>,
    pub content: String,
    pub content_ref: Option<String>,
    pub title: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub speed: f64,
    pub provider_id: Option<String>,
    pub total_chars: i64,
    pub total_tokens: i64,
}

impl Task {
    pub fn new(req: CreateTaskRequest) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            task_type: req.task_type,
            status: TaskStatus::Pending,
            batch_id: req.batch_id,
            group_id: None,
            content: req.content,
            content_ref: req.content_ref,
            title: req.title,
            voice: req.voice,
            model: req.model,
            style: req.style,
            speed: req.speed,
            provider_id: req.provider_id,
            total_chars: req.total_chars,
            total_tokens: req.total_tokens,
            total_chunks: 0,
            done_chunks: 0,
            failed_chunks: 0,
            output_path: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    pub fn transition_to(&mut self, status: TaskStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(&status) {
            return Err(AppError::InvalidInput(
                format!("Cannot transition from {:?} to {:?}", self.status, status)
            ));
        }
        let is_terminal = matches!(&status, TaskStatus::Done | TaskStatus::Cancelled);
        self.status = status;
        self.updated_at = Utc::now();
        if is_terminal {
            self.completed_at = Some(Utc::now());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_valid_transitions() {
        let transitions = [
            (TaskStatus::Pending, TaskStatus::Queued),
            (TaskStatus::Queued, TaskStatus::Chunking),
            (TaskStatus::Queued, TaskStatus::Processing),  // worker picks first chunk
            (TaskStatus::Chunking, TaskStatus::Processing),
            (TaskStatus::Processing, TaskStatus::Merging),
            (TaskStatus::Merging, TaskStatus::Done),
            (TaskStatus::Merging, TaskStatus::MergingFailed),
            (TaskStatus::MergingFailed, TaskStatus::Merging),
            (TaskStatus::MergingFailed, TaskStatus::Done),
            (TaskStatus::Processing, TaskStatus::Failed),
            (TaskStatus::Failed, TaskStatus::Queued),
            (TaskStatus::Processing, TaskStatus::Paused),
            (TaskStatus::Paused, TaskStatus::Processing),
            (TaskStatus::Pending, TaskStatus::Cancelled),
            (TaskStatus::Processing, TaskStatus::Cancelled),
            (TaskStatus::MergingFailed, TaskStatus::Cancelled),
        ];
        for (from, to) in &transitions {
            assert!(from.can_transition_to(to), "{:?} -> {:?} should be valid", from, to);
        }
    }

    #[test]
    fn test_task_status_invalid_transitions() {
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Done)); // skip states
        assert!(!TaskStatus::Done.can_transition_to(&TaskStatus::Processing)); // terminal
        assert!(!TaskStatus::Cancelled.can_transition_to(&TaskStatus::Pending)); // terminal
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Failed)); // skip processing
    }

    #[test]
    fn test_task_creation() {
        let req = CreateTaskRequest {
            task_type: TaskType::Single,
            batch_id: None,
            content: "测试内容".into(),
            content_ref: None,
            title: "Test Task".into(),
            voice: "default_voice".into(),
            model: "default_model".into(),
            style: None,
            speed: 1.0,
            provider_id: None,
            total_chars: 100,
            total_tokens: 50,
        };
        let task = Task::new(req);
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.completed_at.is_none());
    }

    #[test]
    fn test_task_transition() {
        let req = CreateTaskRequest {
            task_type: TaskType::Single, batch_id: None,
            content: "hello".into(), content_ref: None,
            title: "t".into(), voice: "v".into(), model: "m".into(),
            style: None, speed: 1.0, provider_id: None, total_chars: 10, total_tokens: 5,
        };
        let mut task = Task::new(req);
        task.transition_to(TaskStatus::Queued).unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
        task.transition_to(TaskStatus::Chunking).unwrap();
        assert_eq!(task.status, TaskStatus::Chunking);
    }

    #[test]
    fn test_task_invalid_transition_returns_error() {
        let req = CreateTaskRequest {
            task_type: TaskType::Single, batch_id: None,
            content: "hello".into(), content_ref: None,
            title: "t".into(), voice: "v".into(), model: "m".into(),
            style: None, speed: 1.0, provider_id: None, total_chars: 10, total_tokens: 5,
        };
        let mut task = Task::new(req);
        let result = task.transition_to(TaskStatus::Done); // Pending -> Done invalid
        assert!(result.is_err());
        assert_eq!(task.status, TaskStatus::Pending); // unchanged
    }

    #[test]
    fn test_task_done_sets_completed_at() {
        let req = CreateTaskRequest {
            task_type: TaskType::Single, batch_id: None,
            content: "hello".into(), content_ref: None,
            title: "t".into(), voice: "v".into(), model: "m".into(),
            style: None, speed: 1.0, provider_id: None, total_chars: 10, total_tokens: 5,
        };
        let mut task = Task::new(req);
        task.transition_to(TaskStatus::Queued).unwrap();
        task.transition_to(TaskStatus::Chunking).unwrap();
        task.transition_to(TaskStatus::Processing).unwrap();
        task.transition_to(TaskStatus::Merging).unwrap();
        task.transition_to(TaskStatus::Done).unwrap();
        assert!(task.completed_at.is_some());
    }
}
