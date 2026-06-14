use crate::shared::id::Id;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DomainEvent {
    FileParsed { batch_id: Id, filename: String, seq: i32, chars: i64, tokens: i64 },
    ParsingComplete { batch_id: Id, total: usize, parsed: usize, failed: usize },
    TaskEnqueued { task_id: Id, batch_id: Option<Id> },
    ChunkCompleted { chunk_id: Id, task_id: Id, seq: i32, audio_path: String, duration: f64 },
    ChunkFailed { chunk_id: Id, task_id: Id, seq: i32, error: String, retry_count: i32 },
    AllChunksDone { task_id: Id, total_chunks: i32 },
    TaskCompleted { task_id: Id, batch_id: Option<Id>, output_path: String, duration: f64 },
    TaskFailed { task_id: Id, error: String },
    BatchPaused { batch_id: Id },
    BatchResumed { batch_id: Id },
    BatchCancelled { batch_id: Id },
    BatchCompleted { batch_id: Id },
    BatchFailed { batch_id: Id, error: String, failed_count: i32 },
    GroupCompleted { group_id: Id, batch_id: Id },
    GroupFailed { group_id: Id, batch_id: Id, error: String },
    TaskStatusChanged { task_id: Id, batch_id: Option<Id>, status: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization_roundtrip() {
        let events = vec![
            DomainEvent::FileParsed {
                batch_id: Id::new(), filename: "test.txt".into(), seq: 1, chars: 100, tokens: 50,
            },
            DomainEvent::ParsingComplete {
                batch_id: Id::new(), total: 5, parsed: 4, failed: 1,
            },
            DomainEvent::TaskEnqueued {
                task_id: Id::new(), batch_id: Some(Id::new()),
            },
            DomainEvent::ChunkCompleted {
                chunk_id: Id::new(), task_id: Id::new(), seq: 1,
                audio_path: "/tmp/test.wav".into(), duration: 2.5,
            },
            DomainEvent::ChunkFailed {
                chunk_id: Id::new(), task_id: Id::new(), seq: 1,
                error: "API error".into(), retry_count: 2,
            },
            DomainEvent::AllChunksDone {
                task_id: Id::new(), total_chunks: 3,
            },
            DomainEvent::TaskCompleted {
                task_id: Id::new(), batch_id: None,
                output_path: "/tmp/output.wav".into(), duration: 10.0,
            },
            DomainEvent::TaskFailed {
                task_id: Id::new(), error: "error".into(),
            },
            DomainEvent::BatchPaused { batch_id: Id::new() },
            DomainEvent::BatchResumed { batch_id: Id::new() },
            DomainEvent::BatchCancelled { batch_id: Id::new() },
            DomainEvent::BatchCompleted { batch_id: Id::new() },
            DomainEvent::BatchFailed {
                batch_id: Id::new(), error: "batch error".into(), failed_count: 2,
            },
            DomainEvent::GroupCompleted { group_id: Id::new(), batch_id: Id::new() },
            DomainEvent::GroupFailed {
                group_id: Id::new(), batch_id: Id::new(), error: "group error".into(),
            },
            DomainEvent::TaskStatusChanged {
                task_id: Id::new(), batch_id: Some(Id::new()), status: "queued".into(),
            },
        ];
        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
            assert!(
                std::mem::discriminant(event) == std::mem::discriminant(&deserialized),
                "Discriminant mismatch for event: {}",
                json
            );
        }
    }
}
