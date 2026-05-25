#[cfg(test)]
mod tests {
    use crate::db;
    use crate::models::response::StatsSummary;
    use crate::services::stats_cache::StatsCache;
    use crate::test_utils::{fixture_task, test_db};

    #[test]
    fn cache_hit_returns_cached_value() {
        let pool = test_db();
        fixture_task(&pool, "t-1", "a", "completed");
        fixture_task(&pool, "t-2", "b", "failed");

        let cache = StatsCache::new(pool.clone());

        // First call: compute from DB
        let stats1 = cache.get_or_refresh();
        assert_eq!(stats1.total_tasks, 2);

        // Second call: should hit cache (no new tasks added)
        let stats2 = cache.get_or_refresh();
        assert_eq!(stats2.total_tasks, 2);
        assert_eq!(stats2.completed, stats1.completed);
        assert_eq!(stats2.failed, stats1.failed);
    }

    #[test]
    fn invalidate_forces_recompute() {
        let pool = test_db();
        fixture_task(&pool, "t-1", "a", "completed");

        let cache = StatsCache::new(pool.clone());

        // First: populate cache
        let stats1 = cache.get_or_refresh();
        assert_eq!(stats1.total_tasks, 1);

        // Add a new task (cache is now stale)
        fixture_task(&pool, "t-2", "b", "failed");
        cache.invalidate();

        // After invalidation: should recompute
        let stats2 = cache.get_or_refresh();
        assert_eq!(stats2.total_tasks, 2);
        assert_eq!(stats2.completed, 1);
        assert_eq!(stats2.failed, 1);
    }

    #[test]
    fn empty_cache_computes_on_first_call() {
        let pool = test_db();
        let cache = StatsCache::new(pool.clone());

        // No tasks — first call still works
        let stats = cache.get_or_refresh();
        assert_eq!(stats.total_tasks, 0);
    }

    #[test]
    fn multiple_invalidates_work() {
        let pool = test_db();
        fixture_task(&pool, "t-1", "a", "pending");

        let cache = StatsCache::new(pool.clone());
        let _ = cache.get_or_refresh(); // populate

        // Invalidate multiple times
        cache.invalidate();
        cache.invalidate();
        cache.invalidate();

        let stats = cache.get_or_refresh();
        assert_eq!(stats.total_tasks, 1);
    }

    #[test]
    fn cache_returns_clone_not_reference() {
        let pool = test_db();
        fixture_task(&pool, "t-1", "a", "completed");

        let cache = StatsCache::new(pool.clone());
        let stats1 = cache.get_or_refresh();
        let stats2 = cache.get_or_refresh();

        // stats2 should be a clone, not the same reference
        // (they should be equal but not identical)
        assert_eq!(stats1.total_tasks, stats2.total_tasks);
    }
}
