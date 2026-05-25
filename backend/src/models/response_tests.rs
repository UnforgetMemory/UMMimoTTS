#[cfg(test)]
mod tests {
    use crate::models::response::*;
    use crate::models::task::TaskStatus;
    use crate::models::batch::GroupStatus;

    // ── TaskSummary ─────────────────────────────────────────────────

    #[test]
    fn task_summary_serialization() {
        let summary = TaskSummary {
            id: "task-1".into(),
            custom_title: Some("Hello".into()),
            status: TaskStatus::Pending,
            voice: Some("voice-a".into()),
            char_count: 100,
            token_count: 50,
            progress: 0.5,
            has_audio: false,
            group_id: None,
            created_at: "2024-01-01T00:00:00Z".into(),
            completed_at: None,
            elapsed_secs: Some(30.0),
            current_chunk: Some(1),
            total_chunks: Some(5),
        };

        let json = serde_json::to_string(&summary).expect("serialization");
        let deser: TaskSummary = serde_json::from_str(&json).expect("deserialization");
        assert_eq!(deser.id, "task-1");
        assert_eq!(deser.custom_title.as_deref(), Some("Hello"));
        assert_eq!(deser.status, TaskStatus::Pending);
        assert_eq!(deser.char_count, 100);
    }

    #[test]
    fn task_summary_roundtrip_with_completion() {
        let summary = TaskSummary {
            id: "done-1".into(),
            custom_title: Some("Done".into()),
            status: TaskStatus::Completed,
            voice: None,
            char_count: 200,
            token_count: 100,
            progress: 1.0,
            has_audio: true,
            group_id: Some("g-1".into()),
            created_at: "2024-01-01T00:00:00Z".into(),
            completed_at: Some("2024-01-01T00:05:00Z".into()),
            elapsed_secs: Some(300.0),
            current_chunk: None,
            total_chunks: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        let back: TaskSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, TaskStatus::Completed);
        assert!(back.has_audio);
        assert_eq!(back.group_id.as_deref(), Some("g-1"));
        assert_eq!(back.completed_at.as_deref(), Some("2024-01-01T00:05:00Z"));
    }

    // ── PaginatedResponse ─────────────────────────────────────────

    #[test]
    fn paginated_response_new_first_page() {
        let items = vec![1, 2, 3];
        let resp = PaginatedResponse::new(items, 10, 0, 3);
        assert_eq!(resp.page, 0);
        assert_eq!(resp.per_page, 3);
        assert_eq!(resp.total, 10);
        assert_eq!(resp.total_pages, 4);
        assert_eq!(resp.items.len(), 3);
    }

    #[test]
    fn paginated_response_exact_fit() {
        let items = vec![1, 2, 3, 4, 5];
        let resp = PaginatedResponse::new(items, 5, 0, 5);
        assert_eq!(resp.total_pages, 1);
    }

    #[test]
    fn paginated_response_empty() {
        let items: Vec<i32> = vec![];
        let resp = PaginatedResponse::new(items, 0, 0, 50);
        assert_eq!(resp.total_pages, 0);
        assert!(resp.items.is_empty());
    }

    #[test]
    fn paginated_response_zero_per_page() {
        let items: Vec<i32> = vec![1];
        let resp = PaginatedResponse::new(items, 1, 0, 0);
        assert_eq!(resp.total_pages, 1); // safe default
    }

    // ── StatsSummary ──────────────────────────────────────────────

    #[test]
    fn stats_summary_serialization() {
        let stats = StatsSummary {
            total_tasks: 10,
            completed: 5,
            failed: 2,
            processing: 3,
            total_tokens: 1500,
            total_chars: 3000,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deser: StatsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_tasks, 10);
        assert_eq!(deser.completed, 5);
        assert_eq!(deser.total_chars, 3000);
    }

    // ── TaskListQuery ─────────────────────────────────────────────

    #[test]
    fn task_list_query_deser_full() {
        let json = r#"{"page":0,"per_page":50,"status":"pending","search":"hello","sort":"created_desc","group_id":"g-1"}"#;
        let query: TaskListQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.page, Some(0));
        assert_eq!(query.per_page, Some(50));
        assert_eq!(query.status.as_deref(), Some("pending"));
        assert_eq!(query.search.as_deref(), Some("hello"));
        assert_eq!(query.sort.as_deref(), Some("created_desc"));
        assert_eq!(query.group_id.as_deref(), Some("g-1"));
    }

    #[test]
    fn task_list_query_deser_minimal() {
        let json = r#"{}"#;
        let query: TaskListQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.page, None);
        assert_eq!(query.per_page, None);
        assert_eq!(query.status, None);
        assert_eq!(query.search, None);
        assert_eq!(query.sort, None);
        assert_eq!(query.group_id, None);
    }
}
