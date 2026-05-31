//! Chunk Recovery — resets orphaned Processing chunks.
//!
//! Periodically scans for chunks stuck in `Processing` status beyond a
//! time threshold and resets them to `Pending` so the ChunkQueue workers
//! can pick them up again. This handles scenarios where:
//! - A spawned tokio task panicked before marking the chunk done/failed
//! - The server crashed mid-processing
//! - Events were lost due to broadcast channel lag
//!
//! Unlike the TaskWatchdog (which operates at task level), this operates
//! at chunk level and ensures chunks don't silently disappear.

use crate::infra::persistence::chunk_repo::ChunkRepo;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Configuration for chunk recovery.
pub struct ChunkRecoveryConfig {
    /// How often to run recovery (default: 30 seconds).
    pub check_interval: Duration,
    /// How long a chunk can be in Processing before considered stale (default: 2 minutes).
    pub stale_threshold: Duration,
}

impl Default for ChunkRecoveryConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            stale_threshold: Duration::from_secs(120), // 2 minutes
        }
    }
}

/// Periodically resets orphaned Processing chunks to Pending.
pub struct ChunkRecovery {
    chunk_repo: Arc<dyn ChunkRepo>,
    config: ChunkRecoveryConfig,
}

impl ChunkRecovery {
    pub fn new(chunk_repo: Arc<dyn ChunkRepo>, config: ChunkRecoveryConfig) -> Self {
        Self { chunk_repo, config }
    }

    /// Start the recovery loop. Returns a JoinHandle.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        let interval = self.config.check_interval;
        let stale_secs = self.config.stale_threshold.as_secs();
        info!(
            "ChunkRecovery started — check every {:?}, stale threshold {}s",
            interval, stale_secs
        );

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            timer.tick().await; // first tick completes immediately

            loop {
                timer.tick().await;
                self.run_recovery().await;
            }
        })
    }

    async fn run_recovery(&self) {
        let stale_minutes = self.config.stale_threshold.as_secs() as i64 / 60;
        let stale_minutes = stale_minutes.max(1); // at least 1 minute

        match self
            .chunk_repo
            .reset_stale_processing_to_pending(stale_minutes)
        {
            Ok(0) => {} // nothing to do
            Ok(count) => {
                warn!(
                    "ChunkRecovery: reset {} stale Processing chunks back to Pending",
                    count
                );
            }
            Err(e) => {
                error!("ChunkRecovery: failed to reset stale chunks: {e}");
            }
        }
    }
}
