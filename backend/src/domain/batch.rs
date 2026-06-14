use crate::shared::id::Id;
use crate::shared::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BatchStatus {
    Preparing,   // uploading files, editing items
    Queued,      // submitted, Tasks created
    Processing,  // ≥1 Task in processing
    Paused,      // paused by user
    Completed,   // all child Tasks terminal
    Failed,      // all child Tasks Failed/Cancelled
    Cancelled,   // cancelled by user
}

impl BatchStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::Preparing, Self::Queued)
            | (Self::Preparing, Self::Cancelled)
            | (Self::Queued, Self::Processing)
            | (Self::Queued, Self::Cancelled)
            | (Self::Processing, Self::Completed)
            | (Self::Processing, Self::Failed)
            | (Self::Processing, Self::Paused)
            | (Self::Processing, Self::Cancelled)
            | (Self::Paused, Self::Processing)
            | (Self::Paused, Self::Cancelled)
            | (Self::Failed, Self::Queued)  // retry all
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: Id,
    pub status: BatchStatus,
    pub title: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub speed: f64,
    pub total_items: i32,
    pub total_chars: i64,
    pub total_tokens: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Batch {
    pub fn new(title: String, voice: String, model: String, style: Option<String>, speed: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            status: BatchStatus::Preparing,
            title,
            voice,
            model,
            style,
            speed,
            total_items: 0,
            total_chars: 0,
            total_tokens: 0,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    pub fn transition_to(&mut self, status: BatchStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(&status) {
            return Err(AppError::InvalidInput(
                format!("Cannot transition from {:?} to {:?}", self.status, status)
            ));
        }
        let is_terminal = matches!(&status, BatchStatus::Completed | BatchStatus::Cancelled | BatchStatus::Failed);
        self.status = status;
        self.updated_at = Utc::now();
        if is_terminal {
            self.completed_at = Some(Utc::now());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPendingItem {
    // Core data
    pub seq: i32,
    pub filename: String,
    pub content: String,
    pub total_chars: i64,
    pub token_estimate: i64,

    // Overrides (from user edit)
    pub custom_voice: Option<String>,
    pub custom_model: Option<String>,
    pub custom_title: Option<String>,
    pub custom_style: Option<String>,
    pub custom_speed: Option<f64>,
    pub priority: i32,

    // Computed effective values (inherit from batch if override not set)
    pub effective_voice: String,
    pub effective_model: String,
    pub effective_title: String,
    pub effective_style: Option<String>,
    pub effective_speed: f64,
}

impl BatchPendingItem {
    pub fn new_for_test(
        batch: &Batch,
        filename: &str,
        content: &str,
        custom_overrides: Option<(Option<String>, Option<String>)>,
    ) -> Self {
        let (custom_voice, custom_title) = custom_overrides.unwrap_or((None, None));
        Self {
            seq: 0,
            filename: filename.to_string(),
            content: content.to_string(),
            total_chars: content.len() as i64,
            token_estimate: content.len() as i64 / 2,
            custom_voice: custom_voice.clone(),
            custom_model: None,
            custom_title: custom_title.clone(),
            custom_style: None,
            custom_speed: None,
            priority: 0,
            effective_voice: custom_voice.clone().unwrap_or_else(|| batch.voice.clone()),
            effective_model: batch.model.clone(),
            effective_title: custom_title.clone().unwrap_or_else(|| filename.to_string()),
            effective_style: batch.style.clone(),
            effective_speed: batch.speed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_status_valid_transitions() {
        let transitions = [
            (BatchStatus::Preparing, BatchStatus::Queued),
            (BatchStatus::Preparing, BatchStatus::Cancelled),
            (BatchStatus::Queued, BatchStatus::Processing),
            (BatchStatus::Processing, BatchStatus::Completed),
            (BatchStatus::Processing, BatchStatus::Paused),
            (BatchStatus::Paused, BatchStatus::Processing),
            (BatchStatus::Failed, BatchStatus::Queued),
        ];
        for (from, to) in &transitions {
            assert!(from.can_transition_to(to), "{:?} -> {:?} should be valid", from, to);
        }
    }

    #[test]
    fn test_batch_status_invalid_transitions() {
        assert!(!BatchStatus::Preparing.can_transition_to(&BatchStatus::Completed));
        assert!(!BatchStatus::Completed.can_transition_to(&BatchStatus::Processing));
        assert!(!BatchStatus::Cancelled.can_transition_to(&BatchStatus::Preparing));
    }

    #[test]
    fn test_batch_creation() {
        let batch = Batch::new("Test Batch".into(), "v1".into(), "m1".into(), None, 1.0);
        assert_eq!(batch.status, BatchStatus::Preparing);
        assert_eq!(batch.total_items, 0);
    }

    #[test]
    fn test_batch_done_sets_completed_at() {
        let mut batch = Batch::new("Test".into(), "v".into(), "m".into(), None, 1.0);
        batch.transition_to(BatchStatus::Queued).unwrap();
        batch.transition_to(BatchStatus::Processing).unwrap();
        batch.transition_to(BatchStatus::Completed).unwrap();
        assert!(batch.completed_at.is_some());
    }

    #[test]
    fn test_pending_item_effective_inherits_batch() {
        let batch = Batch::new("Test".into(), "v1".into(), "m1".into(), None, 1.0);
        let item = BatchPendingItem::new_for_test(&batch, "file.txt", "content", None);
        assert_eq!(item.effective_voice, batch.voice);
        assert_eq!(item.effective_title, "file.txt");
    }

    #[test]
    fn test_pending_item_effective_uses_custom() {
        let batch = Batch::new("Test".into(), "v1".into(), "m1".into(), None, 1.0);
        let item = BatchPendingItem::new_for_test(
            &batch, "file.txt", "content",
            Some((Some("custom_v".into()), Some("custom_title".into()))),
        );
        assert_eq!(item.effective_voice, "custom_v");
        assert_eq!(item.effective_title, "custom_title");
    }
}
