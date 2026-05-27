use crate::shared::id::Id;
use crate::shared::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GroupStatus {
    Pending,    // created, not queued
    Queued,     // submitted to TaskQueue
    Processing, // all child tasks in queue
    Paused,     // all tasks paused
    Completed,  // all child tasks Done
    Failed,     // any child task Failed
    Cancelled,  // cancelled by user
}

impl GroupStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::Pending, Self::Queued)
            | (Self::Queued, Self::Processing)
            | (Self::Processing, Self::Completed)
            | (Self::Processing, Self::Failed)
            | (Self::Processing, Self::Paused)
            | (Self::Processing, Self::Cancelled)
            | (Self::Paused, Self::Processing)
            | (Self::Paused, Self::Cancelled)
            | (Self::Failed, Self::Queued)  // retry
            | (Self::Failed, Self::Cancelled)
            | (Self::Completed, Self::Queued)  // re-run
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Id,
    pub batch_id: Id,
    pub status: GroupStatus,
    pub title: String,
    pub total_tasks: i32,
    pub done_tasks: i32,
    pub failed_tasks: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Group {
    pub fn new(batch_id: Id, title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            batch_id,
            status: GroupStatus::Pending,
            title,
            total_tasks: 0,
            done_tasks: 0,
            failed_tasks: 0,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    pub fn transition_to(&mut self, status: GroupStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(&status) {
            return Err(AppError::InvalidInput(
                format!("Cannot transition from {:?} to {:?}", self.status, status)
            ));
        }
        let is_terminal = matches!(&status, GroupStatus::Completed | GroupStatus::Cancelled);
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
    fn test_group_status_valid_transitions() {
        let transitions = [
            (GroupStatus::Pending, GroupStatus::Queued),
            (GroupStatus::Queued, GroupStatus::Processing),
            (GroupStatus::Processing, GroupStatus::Completed),
            (GroupStatus::Processing, GroupStatus::Paused),
            (GroupStatus::Paused, GroupStatus::Processing),
            (GroupStatus::Failed, GroupStatus::Queued),
            (GroupStatus::Completed, GroupStatus::Queued),
        ];
        for (from, to) in &transitions {
            assert!(from.can_transition_to(to), "{:?} -> {:?} should be valid", from, to);
        }
    }

    #[test]
    fn test_group_status_invalid_transitions() {
        assert!(!GroupStatus::Pending.can_transition_to(&GroupStatus::Completed));
        assert!(!GroupStatus::Cancelled.can_transition_to(&GroupStatus::Processing));
    }

    #[test]
    fn test_group_creation() {
        let group = Group::new(Id::new(), "Test Group".into());
        assert_eq!(group.status, GroupStatus::Pending);
        assert_eq!(group.total_tasks, 0);
    }
}
