//! Server-Sent Events bus.
//!
//! Full implementation with per-channel broadcast fan-out.
//! Each logical channel (e.g. `batch:{id}`, `task:{id}`) has its own
//! `broadcast::Sender`.  Subscribers receive events in real time.

#![allow(dead_code)]

use crate::domain::events::DomainEvent;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Channel capacity per subscriber (events buffered before lag / drop).
const CHANNEL_CAPACITY: usize = 256;

/// Manages per-channel event broadcast.
///
/// Channels are created lazily on first publish or subscribe.
///
/// ```ignore
/// let bus = SseBus::new();
/// bus.publish("batch:abc", &DomainEvent::BatchCompleted { batch_id });
/// let mut rx = bus.subscribe("batch:abc");
/// ```
#[derive(Clone)]
pub struct SseBus {
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<DomainEvent>>>>,
}

impl SseBus {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish a domain event on the given channel.
    ///
    /// Creates the channel lazily if it does not exist yet.
    pub fn publish(&self, channel: &str, event: &DomainEvent) {
        let tx = {
            let read = self.channels.read();
            read.get(channel).cloned()
        };
        match tx {
            Some(tx) => {
                let _ = tx.send(event.clone());
            }
            None => {
                // Create the channel lazily
                let (new_tx, _) = broadcast::channel(CHANNEL_CAPACITY);
                let _ = new_tx.send(event.clone());
                let mut write = self.channels.write();
                // Avoid double-insert
                write.entry(channel.to_string()).or_insert(new_tx);
            }
        }
    }

    /// Subscribe to a channel.
    ///
    /// Creates the channel if it does not exist yet.
    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<DomainEvent> {
        let mut write = self.channels.write();
        let tx = write
            .entry(channel.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
                tx
            });
        tx.subscribe()
    }
}

/// Spawn a background task that bridges the application-level event_tx
/// (a `broadcast::Sender<DomainEvent>`) to the SSE bus so that any
/// `DomainEvent` published by the system is automatically fanned out
/// to all SSE subscribers on the matching channel.
///
/// The bridge maps:
/// - `batch:{batch_id}` — batch-level events
/// - `task:{task_id}`   — task-level events
pub fn spawn_sse_bridge(
    mut event_rx: broadcast::Receiver<DomainEvent>,
    sse_bus: Arc<SseBus>,
) {
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    // Derive channel(s) from the event and publish
                    let channels = event_channels(&event);
                    for ch in channels {
                        sse_bus.publish(&ch, &event);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("sse-bridge: event channel closed, exiting");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("sse-bridge: lagged by {n} events");
                }
            }
        }
    });
}

/// Map a domain event to the SSE channel(s) it should be published on.
fn event_channels(event: &DomainEvent) -> Vec<String> {
    match event {
        DomainEvent::FileParsed { batch_id, .. }
        | DomainEvent::ParsingComplete { batch_id, .. }
        | DomainEvent::BatchCompleted { batch_id }
        | DomainEvent::BatchFailed { batch_id, .. }
        | DomainEvent::GroupCompleted { batch_id, .. }
        | DomainEvent::GroupFailed { batch_id, .. } => {
            vec![format!("batch:{batch_id}")]
        }
        DomainEvent::TaskEnqueued { task_id, batch_id: Some(bid), .. }
        | DomainEvent::TaskCompleted { task_id, batch_id: Some(bid), .. } => {
            vec![format!("batch:{bid}"), format!("task:{task_id}")]
        }
        DomainEvent::TaskFailed { task_id, .. } => {
            vec![format!("task:{task_id}")]
        }
        DomainEvent::TaskEnqueued { task_id, batch_id: None, .. }
        | DomainEvent::TaskCompleted { task_id, batch_id: None, .. } => {
            vec![format!("task:{task_id}")]
        }
        DomainEvent::ChunkCompleted { task_id, .. }
        | DomainEvent::ChunkFailed { task_id, .. }
        | DomainEvent::AllChunksDone { task_id, .. } => {
            vec![format!("task:{task_id}")]
        }
    }
}
