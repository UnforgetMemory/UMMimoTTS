#[cfg(test)]
mod tests {
    use crate::db;
    use crate::models::task::TaskStatus;
    use crate::test_utils::{create_test_app_state, fixture_task, test_db};

    // ── Task CRUD ──────────────────────────────────────────────────

    #[test]
    fn insert_and_get_task() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "Hello World", "pending");
        let fetched = db::get_task_from_db(&pool, "task-1").expect("task should exist");
        assert_eq!(fetched.id, "task-1");
        assert_eq!(fetched.custom_title.as_deref(), Some("Hello World"));
        assert_eq!(fetched.status, TaskStatus::Pending);
    }

    #[test]
    fn get_nonexistent_task_returns_none() {
        let pool = test_db();
        let result = db::get_task_from_db(&pool, "no-such-id");
        assert!(result.is_none());
    }

    #[test]
    fn update_task_changes_status() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "Title", "pending");
        let mut task = db::get_task_from_db(&pool, "task-1").unwrap();
        task.status = TaskStatus::Completed;
        db::update_task(&pool, &task);
        let updated = db::get_task_from_db(&pool, "task-1").unwrap();
        assert_eq!(updated.status, TaskStatus::Completed);
    }

    #[test]
    fn update_task_changes_title() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "Old Title", "pending");
        let mut task = db::get_task_from_db(&pool, "task-1").unwrap();
        task.custom_title = Some("New Title".to_string());
        db::update_task(&pool, &task);
        let updated = db::get_task_from_db(&pool, "task-1").unwrap();
        assert_eq!(updated.custom_title.as_deref(), Some("New Title"));
    }

    #[test]
    fn delete_existing_task() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "Title", "pending");
        db::delete_task(&pool, "task-1");
        assert!(db::get_task_from_db(&pool, "task-1").is_none());
    }

    #[test]
    fn delete_nonexistent_task_is_noop() {
        let pool = test_db();
        db::delete_task(&pool, "no-such-id");
        // Should not panic
    }

    // ── Pagination ─────────────────────────────────────────────────

    #[test]
    fn paginated_list_first_page() {
        let pool = test_db();
        for i in 0..10 {
            fixture_task(&pool, &format!("task-{}", i), &format!("Task {}", i), "pending");
        }

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(0, 3, None, None, None, None);
        assert_eq!(tasks.len(), 3);
        assert_eq!(total, 10);
    }

    #[test]
    fn paginated_list_last_page() {
        let pool = test_db();
        for i in 0..10 {
            fixture_task(&pool, &format!("task-{}", i), &format!("Task {}", i), "pending");
        }

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(3, 3, None, None, None, None);
        assert_eq!(tasks.len(), 1);
        assert_eq!(total, 10);
    }

    #[test]
    fn paginated_list_all_in_one_page() {
        let pool = test_db();
        for i in 0..10 {
            fixture_task(&pool, &format!("task-{}", i), &format!("Task {}", i), "pending");
        }

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(0, 100, None, None, None, None);
        assert_eq!(tasks.len(), 10);
        assert_eq!(total, 10);
    }

    #[test]
    fn paginated_list_out_of_range_page_returns_empty() {
        let pool = test_db();
        for i in 0..5 {
            fixture_task(&pool, &format!("task-{}", i), &format!("Task {}", i), "pending");
        }

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(10, 3, None, None, None, None);
        assert!(tasks.is_empty());
        assert_eq!(total, 5);
    }

    // ── Filtering ──────────────────────────────────────────────────

    #[test]
    fn filter_by_status() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "A", "pending");
        fixture_task(&pool, "task-2", "B", "completed");
        fixture_task(&pool, "task-3", "C", "pending");

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(0, 50, Some("pending"), None, None, None);
        assert_eq!(tasks.len(), 2);
        assert_eq!(total, 2);
    }

    #[test]
    fn filter_by_search() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "Hello World", "pending");
        fixture_task(&pool, "task-2", "Goodbye", "pending");

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(0, 50, None, Some("Hello"), None, None);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-1");
    }

    #[test]
    fn filter_by_group_id() {
        let pool = test_db();
        fixture_task(&pool, "task-1", "A", "pending");
        fixture_task(&pool, "task-2", "B", "pending");

        // Attach task-1 to group-X by updating its group_id
        let mut task = db::get_task_from_db(&pool, "task-1").unwrap();
        task.group_id = Some("group-X".to_string());
        db::update_task(&pool, &task);

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(0, 50, None, None, None, Some("group-X"));
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-1");
    }

    // ── Empty database ─────────────────────────────────────────────

    #[test]
    fn empty_db_returns_empty_list() {
        let pool = test_db();
        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_tasks_paginated(0, 50, None, None, None, None);
        assert!(tasks.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn empty_db_get_returns_none() {
        let pool = test_db();
        assert!(db::get_task_from_db(&pool, "any-id").is_none());
    }

    // ── Group operations ──────────────────────────────────────────

    #[test]
    fn insert_and_get_group() {
        let pool = test_db();
        crate::test_utils::fixture_group(&pool, "g-1", "Test Group", "pending", vec![]);
        let fetched = db::get_group_from_db(&pool, "g-1").expect("group should exist");
        assert_eq!(fetched.id, "g-1");
        assert_eq!(fetched.name, "Test Group");
    }

    #[test]
    fn list_groups_paginated() {
        let pool = test_db();
        for i in 0..5 {
            crate::test_utils::fixture_group(
                &pool,
                &format!("g-{}", i),
                &format!("Group {}", i),
                "pending",
                vec![],
            );
        }

        let state = create_test_app_state(&pool);
        let (groups, total) = state.list_groups_paginated(0, 3);
        assert_eq!(groups.len(), 3);
        assert_eq!(total, 5);
    }

    #[test]
    fn list_group_tasks_paginated() {
        let pool = test_db();
        // Create tasks belonging to the same group
        for i in 0..5 {
            let mut t = crate::test_utils::fixture_task(
                &pool,
                &format!("t-{}", i),
                &format!("Task {}", i),
                "pending",
            );
            t.group_id = Some("g-1".to_string());
            db::update_task(&pool, &t);
        }

        let state = create_test_app_state(&pool);
        let (tasks, total) = state.list_group_tasks_paginated("g-1", 0, 3);
        assert_eq!(tasks.len(), 3);
        assert_eq!(total, 5);
    }

    // ── Stats ──────────────────────────────────────────────────────

    #[test]
    fn compute_stats_counts() {
        let pool = test_db();
        fixture_task(&pool, "t-1", "A", "pending");
        fixture_task(&pool, "t-2", "B", "pending");
        fixture_task(&pool, "t-3", "C", "completed");
        fixture_task(&pool, "t-4", "D", "failed");

        let stats = db::compute_stats(&pool);
        assert_eq!(stats.total_tasks, 4);
        assert_eq!(stats.processing, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 1);
    }

    #[test]
    fn compute_stats_empty_db() {
        let pool = test_db();
        let stats = db::compute_stats(&pool);
        assert_eq!(stats.total_tasks, 0);
        assert_eq!(stats.processing, 0);
        assert_eq!(stats.completed, 0);
        assert_eq!(stats.failed, 0);
    }
}
