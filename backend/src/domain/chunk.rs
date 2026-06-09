use crate::shared::id::Id;
use crate::shared::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChunkStatus {
    Pending,    // created, awaiting ChunkQueue
    Queued,     // in ChunkQueue
    Processing, // dispatched to MIMO API
    Done,       // completed successfully
    Failed,     // failed, retryable
    Dead,       // exceeded max retries, terminal
}

impl ChunkStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::Pending, Self::Queued)
            | (Self::Queued, Self::Processing)
            | (Self::Processing, Self::Done)
            | (Self::Processing, Self::Failed)
            | (Self::Failed, Self::Queued)   // retry
            | (Self::Failed, Self::Dead)     // max retries exceeded
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Id,
    pub task_id: Id,
    pub seq: i32,
    pub text: String,
    pub status: ChunkStatus,
    pub retry_count: i32,
    pub max_retries: i32,
    pub priority: i64,
    pub audio_path: Option<String>,
    pub duration: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Chunk {
    pub fn new(task_id: Id, seq: i32, text: String) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            task_id,
            seq,
            text,
            status: ChunkStatus::Pending,
            retry_count: 0,
            max_retries: 3,
            priority: 0,
            audio_path: None,
            duration: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    pub fn transition_to(&mut self, status: ChunkStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(&status) {
            return Err(AppError::InvalidInput(
                format!("Cannot transition from {:?} to {:?}", self.status, status)
            ));
        }
        let is_done = status == ChunkStatus::Done;
        if status == ChunkStatus::Failed {
            self.retry_count += 1;
        }
        self.status = status;
        self.updated_at = Utc::now();
        if is_done {
            self.completed_at = Some(Utc::now());
        }
        Ok(())
    }

    pub fn with_retry_config(mut self, max_retries: i32) -> Self {
        self.max_retries = max_retries;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_status_valid() {
        let transitions = [
            (ChunkStatus::Pending, ChunkStatus::Queued),
            (ChunkStatus::Queued, ChunkStatus::Processing),
            (ChunkStatus::Processing, ChunkStatus::Done),
            (ChunkStatus::Processing, ChunkStatus::Failed),
            (ChunkStatus::Failed, ChunkStatus::Queued),
            (ChunkStatus::Failed, ChunkStatus::Dead),
        ];
        for (from, to) in &transitions {
            assert!(from.can_transition_to(to), "{:?} -> {:?} should be valid", from, to);
        }
    }

    #[test]
    fn test_chunk_status_invalid() {
        assert!(!ChunkStatus::Dead.can_transition_to(&ChunkStatus::Queued));
        assert!(!ChunkStatus::Pending.can_transition_to(&ChunkStatus::Done));
        assert!(!ChunkStatus::Done.can_transition_to(&ChunkStatus::Failed));
    }

    #[test]
    fn test_chunk_failed_increments_retry() {
        let mut chunk = Chunk::new(Id::new(), 1, "hello".into());
        chunk.transition_to(ChunkStatus::Queued).unwrap();
        chunk.transition_to(ChunkStatus::Processing).unwrap();
        chunk.transition_to(ChunkStatus::Failed).unwrap();
        assert_eq!(chunk.retry_count, 1);
    }

    #[test]
    fn test_chunk_done_sets_completed_at() {
        let mut chunk = Chunk::new(Id::new(), 1, "hello".into());
        chunk.transition_to(ChunkStatus::Queued).unwrap();
        chunk.transition_to(ChunkStatus::Processing).unwrap();
        chunk.transition_to(ChunkStatus::Done).unwrap();
        assert!(chunk.completed_at.is_some());
    }

    #[test]
    fn test_chunk_with_retry_config() {
        let chunk = Chunk::new(Id::new(), 1, "hello".into()).with_retry_config(5);
        assert_eq!(chunk.max_retries, 5);
    }
}
