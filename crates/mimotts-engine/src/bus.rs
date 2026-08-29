//! Event bus: per-channel broadcast fan-out for SSE.
//!
//! Channel map (ADR-004):
//! - `task:{task_id}`     — task-scoped events
//! - `session:{session_id}` — session-scoped events
//! - `providers`          — provider health/throttle events
//!
//! Bounded (umreview finding): channels are only created on `subscribe`,
//! publishes without subscribers are dropped, and the map is capped with
//! FIFO eviction so a long-running process cannot leak unbounded channels.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use mimotts_core::events::DomainEvent;

const CHANNEL_CAP: usize = 256;
/// Hard cap on live channels (FIFO-evicted). Oldest subscribers get Closed.
const MAX_CHANNELS: usize = 4096;

struct Channels {
    map: HashMap<String, broadcast::Sender<DomainEvent>>,
    order: VecDeque<String>,
}

pub struct Bus {
    channels: RwLock<Channels>,
}

impl Bus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            channels: RwLock::new(Channels {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
        })
    }

    pub fn publish(&self, event: &DomainEvent) {
        for channel in channels_for(event) {
            self.publish_to(&channel, event);
        }
    }

    /// Publishes only into existing channels — never creates one (a publish
    /// with no subscribers must not leak a channel + ring buffer).
    fn publish_to(&self, channel: &str, event: &DomainEvent) {
        let tx = {
            let read = self.channels.read();
            read.map.get(channel).cloned()
        };
        if let Some(tx) = tx {
            let _ = tx.send(event.clone());
        }
    }

    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<DomainEvent> {
        let mut write = self.channels.write();
        if let Some(tx) = write.map.get(channel) {
            return tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(CHANNEL_CAP);
        write.map.insert(channel.to_string(), tx);
        write.order.push_back(channel.to_string());
        while write.order.len() > MAX_CHANNELS {
            if let Some(oldest) = write.order.pop_front() {
                write.map.remove(&oldest);
            }
        }
        rx
    }

    pub fn channel_count(&self) -> usize {
        self.channels.read().map.len()
    }
}

fn channels_for(event: &DomainEvent) -> Vec<String> {
    use DomainEvent::*;
    let (task, session) = match event {
        TaskStatusChanged { task_id, session_id, .. } => (Some(task_id), session_id.as_ref()),
        ChunkCompleted { task_id, .. }
        | ChunkFailed { task_id, .. }
        | AllChunksDone { task_id } => (Some(task_id), None),
        TaskCompleted { task_id, session_id, .. }
        | TaskFailed { task_id, session_id, .. } => (Some(task_id), session_id.as_ref()),
        SessionUpdated { session_id } => (None, Some(session_id)),
        ProviderHealth { .. } => (None, None),
    };
    let mut channels = Vec::with_capacity(3);
    if let Some(t) = task {
        channels.push(format!("task:{t}"));
    }
    if let Some(s) = session {
        channels.push(format!("session:{s}"));
    }
    if matches!(event, ProviderHealth { .. }) {
        channels.push("providers".to_string());
    }
    channels
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimotts_core::domain::Id;

    #[tokio::test]
    async fn task_channel_fanout() {
        let bus = Bus::new();
        let tid = Id::new();
        let mut rx = bus.subscribe(&format!("task:{tid}"));
        bus.publish(&DomainEvent::TaskStatusChanged {
            task_id: tid.clone(),
            session_id: None,
            status: "queued".into(),
        });
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, DomainEvent::TaskStatusChanged { .. }));
    }

    #[tokio::test]
    async fn provider_channel() {
        let bus = Bus::new();
        let mut rx = bus.subscribe("providers");
        bus.publish(&DomainEvent::ProviderHealth {
            provider_id: "xiaomi".into(),
            state: "open".into(),
            retry_after_secs: Some(60),
        });
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, DomainEvent::ProviderHealth { .. }));
    }

    #[tokio::test]
    async fn publish_without_subscribers_creates_nothing() {
        let bus = Bus::new();
        let tid = Id::new();
        bus.publish(&DomainEvent::TaskStatusChanged {
            task_id: tid,
            session_id: None,
            status: "queued".into(),
        });
        assert_eq!(bus.channel_count(), 0);
    }

    #[tokio::test]
    async fn channels_are_capped() {
        let bus = Bus::new();
        for i in 0..(MAX_CHANNELS + 16) {
            let _rx = bus.subscribe(&format!("task:{i}"));
        }
        assert!(bus.channel_count() <= MAX_CHANNELS);
    }
}
