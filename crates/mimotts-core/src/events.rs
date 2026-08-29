//! Domain events — the v4 bus payload.
//!
//! Tagged with `type` (snake_case) for SSE/JSON consumers. The engine fan-out
//! maps events to channels `session:{id}` and `task:{id}` (ADR-004).

use serde::{Deserialize, Serialize};

use crate::domain::Id;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    TaskStatusChanged {
        task_id: Id,
        session_id: Option<Id>,
        status: String,
    },
    ChunkCompleted {
        chunk_id: Id,
        task_id: Id,
        seq: i32,
        audio_path: String,
        duration_ms: i64,
    },
    ChunkFailed {
        chunk_id: Id,
        task_id: Id,
        seq: i32,
        error: String,
    },
    AllChunksDone {
        task_id: Id,
    },
    TaskCompleted {
        task_id: Id,
        session_id: Option<Id>,
        output_path: String,
        duration_ms: i64,
    },
    TaskFailed {
        task_id: Id,
        session_id: Option<Id>,
        error: String,
    },
    SessionUpdated {
        session_id: Id,
    },
    /// Provider throttling health (circuit breaker state transitions) — ADR-012.
    ProviderHealth {
        provider_id: String,
        state: String, // "degraded" | "open" | "half_open" | "closed"
        retry_after_secs: Option<u64>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_roundtrip_tagged() {
        let ev = DomainEvent::TaskStatusChanged {
            task_id: Id::new(),
            session_id: None,
            status: "queued".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"task_status_changed\""), "{json}");
        let back: DomainEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DomainEvent::TaskStatusChanged { .. }));
    }
}
