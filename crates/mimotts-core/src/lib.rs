//! UM-MimoTTS v4 core — pure domain logic, no IO.
//!
//! Layout per ADR (docs/compose/plans/2026-08-28-mimotts-workbench-rebuild.md §5.4):
//! - `domain`   — entities + simplified state machines
//! - `events`   — domain event bus payloads (serde `type`-tagged)
//! - `chunking` — MiMo 8K-context smart chunker (single calibrated estimator)
//! - `audio`    — 24 kHz PCM16LE mono / WAV header math (byte-level, no IO)
//! - `crypto`   — AES-256-GCM secret sealing + token hashing

pub mod audio;
pub mod catalog;
pub mod chunking;
pub mod crypto;
pub mod domain;
pub mod events;

pub use domain::{
    Chunk, ChunkStatus, Id, ProviderKind, Session, SessionStatus, Task, TaskStatus,
};
pub use events::DomainEvent;
