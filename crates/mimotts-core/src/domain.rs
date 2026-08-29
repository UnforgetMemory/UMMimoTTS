//! v4 domain model.
//!
//! ADR-010: flat `sessions → tasks → chunks`, no v2 batch/group tables.
//! Statuses are serialized as bare lowercase strings (`"pending"`) — no JSON quoting.
//! This fixes the v3 nested-quote status storage bug.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// UUIDv7-backed identifier (time-ordered, index friendly).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Id(String);

impl Id {
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }
    pub fn from_str(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(|_| Self(s.to_string()))
            .map_err(|e| format!("invalid id: {e}"))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Task lifecycle (ADR-010 simplified): Pending → Queued → Synthesizing → Merging → Done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Queued,
    Synthesizing,
    Merging,
    Done,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        use TaskStatus::*;
        matches!(
            (self, next),
            (Pending, Queued)
                | (Queued, Synthesizing)
                | (Synthesizing, Merging)
                | (Merging, Done)
                | (Merging, Failed)
                | (Synthesizing, Failed)
                | (Failed, Queued) // retry
                | (Cancelled, Queued) // retry after cancel
                | (Pending, Cancelled)
                | (Queued, Cancelled)
                | (Synthesizing, Cancelled)
                | (Merging, Cancelled)
        )
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

/// Chunk lifecycle: Pending → InFlight → Done | Failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkStatus {
    Pending,
    InFlight,
    Done,
    Failed,
}

impl ChunkStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        use ChunkStatus::*;
        matches!(
            (self, next),
            (Pending, InFlight) | (InFlight, Done) | (InFlight, Failed) | (Failed, Pending)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
}

/// A batch import / workflow session (replaces v2 batch+group).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Id,
    pub name: String,
    pub status: SessionStatus,
    pub total_tasks: i32,
    pub done_tasks: i32,
    pub failed_tasks: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Session {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            name,
            status: SessionStatus::Active,
            total_tasks: 0,
            done_tasks: 0,
            failed_tasks: 0,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }
}

/// A single TTS task. `style` is sent as the `user` message;
/// inline tags `(风格)` / `[笑]` / `(唱歌)` live inside `content` (official spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub session_id: Option<Id>,
    pub title: String,
    pub content: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub status: TaskStatus,
    pub priority: i64,
    pub total_chars: i64,
    pub total_tokens: i64,
    pub total_chunks: i32,
    pub done_chunks: i32,
    pub failed_chunks: i32,
    pub output_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub provider_id: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskInput {
    pub session_id: Option<Id>,
    pub title: String,
    pub content: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub priority: i64,
    pub provider_id: Option<String>,
}

impl Task {
    pub fn new(input: CreateTaskInput) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            session_id: input.session_id,
            title: input.title,
            content: input.content.clone(),
            voice: input.voice,
            model: input.model,
            style: input.style,
            status: TaskStatus::Pending,
            priority: input.priority,
            total_chars: input.content.chars().count() as i64,
            total_tokens: 0,
            total_chunks: 0,
            done_chunks: 0,
            failed_chunks: 0,
            output_path: None,
            duration_ms: None,
            provider_id: input.provider_id,
            error: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }
}

/// One synthesis unit. Text has already been budgeted by the chunker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Id,
    pub task_id: Id,
    pub seq: i32,
    pub text: String,
    pub token_estimate: i64,
    pub status: ChunkStatus,
    pub retry_count: i32,
    pub audio_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Chunk {
    pub fn new(task_id: Id, seq: i32, text: String, token_estimate: i64) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            task_id,
            seq,
            text,
            token_estimate,
            status: ChunkStatus::Pending,
            retry_count: 0,
            audio_path: None,
            duration_ms: None,
            error: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }
}

/// Provider kind. `voiceclone` sample size limits etc. are enforced in the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Xiaomi,
    Custom,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_serialize_bare_lowercase() {
        // v4 contract: no JSON quoting around status values.
        assert_eq!(serde_json::to_string(&TaskStatus::Pending).unwrap(), "\"pending\"");
        assert_eq!(serde_json::to_string(&TaskStatus::Synthesizing).unwrap(), "\"synthesizing\"");
        assert_eq!(serde_json::to_string(&ChunkStatus::InFlight).unwrap(), "\"inflight\"");
        assert_eq!(serde_json::to_string(&SessionStatus::Completed).unwrap(), "\"completed\"");
    }

    #[test]
    fn task_status_valid_transitions() {
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Queued));
        assert!(TaskStatus::Queued.can_transition_to(&TaskStatus::Synthesizing));
        assert!(TaskStatus::Synthesizing.can_transition_to(&TaskStatus::Merging));
        assert!(TaskStatus::Merging.can_transition_to(&TaskStatus::Done));
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Done));
        assert!(TaskStatus::Failed.can_transition_to(&TaskStatus::Queued));
        assert!(TaskStatus::Cancelled.can_transition_to(&TaskStatus::Queued));
    }

    #[test]
    fn chunk_status_transitions() {
        assert!(ChunkStatus::Pending.can_transition_to(&ChunkStatus::InFlight));
        assert!(ChunkStatus::InFlight.can_transition_to(&ChunkStatus::Done));
        assert!(ChunkStatus::Failed.can_transition_to(&ChunkStatus::Pending));
        assert!(!ChunkStatus::Pending.can_transition_to(&ChunkStatus::Done));
    }

    #[test]
    fn id_roundtrip() {
        let id = Id::new();
        let parsed = Id::from_str(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
        assert!(Id::from_str("not-a-uuid").is_err());
    }
}
