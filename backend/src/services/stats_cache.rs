use crate::db::SqlitePool;
use crate::models::response::StatsSummary;
use parking_lot::RwLock;

/// Thread-safe stats cache that refreshes on demand.
/// The cache is invalidated whenever a task changes state.
pub struct StatsCache {
    inner: RwLock<Option<StatsSummary>>,
    pool: SqlitePool,
}

impl StatsCache {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            inner: RwLock::new(None),
            pool,
        }
    }

    /// Return cached stats or compute them if the cache is empty/stale.
    pub fn get_or_refresh(&self) -> StatsSummary {
        // Fast path: check cache
        {
            let cached = self.inner.read();
            if let Some(ref stats) = *cached {
                return stats.clone();
            }
        }

        // Slow path: compute from DB
        let stats = crate::db::compute_stats(&self.pool);

        // Update cache
        *self.inner.write() = Some(stats.clone());
        stats
    }

    /// Invalidate the cache so the next call recomputes.
    pub fn invalidate(&self) {
        *self.inner.write() = None;
    }
}
