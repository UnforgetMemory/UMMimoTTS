#[cfg(test)]
mod tests {
    use crate::db;
    use crate::routes::batch_import::{
        extend_ttl, get_preview, submit, update_item, upload_file,
    };
    use crate::state::app_state::AppState;
    use actix_web::test as actix_test;
    use actix_web::{web, App};
    use std::io::Write;

    // ── Helper: build a multipart/form-data body ───────────────────────
    fn multipart_body(
        boundary: &str,
        files: Vec<(&str, &str, &[u8])>,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        for (field_name, filename, content) in files {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; \
                     name=\"{}\"; filename=\"{}\"\r\n",
                    field_name, filename
                )
                .as_bytes(),
            );
            body.extend_from_slice(
                b"Content-Type: application/octet-stream\r\n\r\n",
            );
            body.extend_from_slice(content);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");
        body
    }

    // ── Helper: create an in-memory zip archive ────────────────────────
    fn create_zip(files: Vec<(&str, &str)>) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        for (name, content) in files {
            zip.start_file(name, zip::write::FileOptions::default())
                .expect("zip start_file");
            zip.write_all(content.as_bytes())
                .expect("zip write content");
        }
        zip.finish().expect("zip finish").into_inner()
    }

    /// Build a test AppState (in-memory DB).
    fn create_state() -> web::Data<AppState> {
        let pool = db::init_db_pool_in_memory();
        web::Data::new(AppState::new_with_pool(pool, String::new()))
    }

    // ═══════════════════════════════════════════════════════════════════
    //  a) Upload single .txt file (5 lines)
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn upload_single_txt_file() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/preview", web::get().to(get_preview))
                .route("/api/v1/batch/extend", web::post().to(extend_ttl))
                .route("/api/v1/batch/items/{index}", web::put().to(update_item))
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        // Upload 5-line file
        let content = b"Line one\nLine two\nLine three\nLine four\nLine five\n";
        let boundary = "test-boundary-a";
        let body = multipart_body(boundary, vec![("file", "items.txt", content)]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let token = json["token"].as_str().expect("token present");
        assert_eq!(token.len(), 36, "token should be a UUID v4");

        let stats = &json["stats"];
        assert_eq!(stats["total_items"], 5, "all five lines should be parsed");
        assert_eq!(stats["valid_items"], 5);
        assert_eq!(stats["error_items"], 0);
        assert!(stats["total_chars"].as_u64().unwrap_or(0) > 0);
    }

    // ═══════════════════════════════════════════════════════════════════
    //  b) Upload empty file → 400
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn upload_empty_file_returns_400() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/preview", web::get().to(get_preview))
                .route("/api/v1/batch/extend", web::post().to(extend_ttl))
                .route("/api/v1/batch/items/{index}", web::put().to(update_item))
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        let boundary = "empty-boundary";
        let body = multipart_body(boundary, vec![("file", "empty.txt", b"")]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "empty file should be rejected");

        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["error"].as_str().is_some(),
            "response should contain an error field"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  c) Preview items after upload (paginated)
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn preview_items_paginated() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/preview", web::get().to(get_preview))
                .route("/api/v1/batch/extend", web::post().to(extend_ttl))
                .route("/api/v1/batch/items/{index}", web::put().to(update_item))
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        // Upload 10 lines
        let content = (1..=10)
            .map(|i| format!("Item {}\n", i))
            .collect::<String>();
        let boundary = "preview-boundary";
        let body = multipart_body(boundary, vec![("file", "preview.txt", content.as_bytes())]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let upload_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = upload_json["token"].as_str().unwrap().to_string();

        // Page 0 with per_page = 3
        let req = actix_test::TestRequest::get()
            .uri(&format!(
                "/api/v1/batch/preview?token={}&page=0&per_page=3",
                token
            ))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let preview: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(preview["total"], 10);
        assert_eq!(preview["page"], 0);
        assert_eq!(preview["per_page"], 3);
        assert_eq!(preview["total_pages"], 4);
        assert_eq!(
            preview["items"].as_array().unwrap().len(),
            3,
            "page 0 should have 3 items"
        );

        // Page 3 (last page) should have 1 item
        let req = actix_test::TestRequest::get()
            .uri(&format!(
                "/api/v1/batch/preview?token={}&page=3&per_page=3",
                token
            ))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        let bytes = actix_test::read_body(resp).await;
        let preview: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(preview["items"].as_array().unwrap().len(), 1);
        assert_eq!(preview["page"], 3);
    }

    // ═══════════════════════════════════════════════════════════════════
    //  d) Preview with invalid token → 404
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn preview_invalid_token_returns_404() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/preview", web::get().to(get_preview)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/api/v1/batch/preview?token=nonexistent&page=0&per_page=10")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    // ═══════════════════════════════════════════════════════════════════
    //  e) Edit item's voice/model/title via PUT
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn edit_item_updates_fields() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/items/{index}", web::put().to(update_item)),
        )
        .await;

        // Upload a file first
        let boundary = "edit-boundary";
        let body = multipart_body(boundary, vec![("file", "edit.txt", b"Line to edit\n")]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        // Update voice, model, and title for item 0
        let update_body = serde_json::json!({
            "token": token,
            "voice": "zh-CN-XiaoxiaoNeural",
            "model": "mimo-v2.5-tts",
            "title": "Edited Title",
            "context": "formal",
        });
        let req = actix_test::TestRequest::put()
            .uri("/api/v1/batch/items/0")
            .set_json(&update_body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["status"], "updated");

        // Verify the update via AppState directly
        let import = state.batch_imports.get_import(&token).unwrap();
        let item = &import.items[0];
        assert_eq!(item.voice.as_deref(), Some("zh-CN-XiaoxiaoNeural"));
        assert_eq!(item.model.as_deref(), Some("mimo-v2.5-tts"));
        assert_eq!(item.title.as_deref(), Some("Edited Title"));
        assert_eq!(item.context.as_deref(), Some("formal"));
    }

    // ═══════════════════════════════════════════════════════════════════
    //  f) Edit nonexistent item → 400
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn edit_nonexistent_item_returns_error() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/items/{index}", web::put().to(update_item)),
        )
        .await;

        // Upload
        let boundary = "edit2-boundary";
        let body = multipart_body(boundary, vec![("file", "edit2.txt", b"Only one\n")]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        // Index 99 is out of bounds
        let update_body = serde_json::json!({
            "token": token,
            "voice": "test-voice",
        });
        let req = actix_test::TestRequest::put()
            .uri("/api/v1/batch/items/99")
            .set_json(&update_body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "out-of-bounds index should error");
    }

    // ═══════════════════════════════════════════════════════════════════
    //  g) Submit batch → verify group_id + DB
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn submit_batch_creates_group() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        // Upload
        let boundary = "submit-boundary";
        let body = multipart_body(
            boundary,
            vec![("file", "submit.txt", b"First task\nSecond task\nThird task\n")],
        );
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        // Submit
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/submit")
            .set_json(&serde_json::json!({
                "token": token,
                "group_name": "My Test Group",
            }))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let group_id = result["group_id"].as_str().unwrap();
        assert_eq!(group_id.len(), 36, "group_id should be a UUID");
        assert_eq!(result["task_count"], 3);
        assert_eq!(
            result["task_ids"].as_array().unwrap().len(),
            3,
            "three task IDs should be returned"
        );

        // Verify tasks exist in AppState (add_task is called by submit)
        let tasks = state.tasks.read();
        let task_ids_arr = result["task_ids"].as_array().unwrap();
        for task_val in task_ids_arr {
            let tid = task_val.as_str().unwrap();
            assert!(tasks.contains_key(tid), "task {} should exist", tid);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  h) Submit with default_voice/default_model overrides
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn submit_with_default_overrides() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        // Upload
        let boundary = "ov-boundary";
        let body = multipart_body(
            boundary,
            vec![("file", "override.txt", b"Task A\nTask B\n")],
        );
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        // Submit with overrides
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/submit")
            .set_json(&serde_json::json!({
                "token": token,
                "group_name": "Overrides",
                "default_voice": "zh-CN-YunxiNeural",
                "default_model": "mimo-v3-tts",
                "default_context": "formal style",
                "default_speed": 1.2,
            }))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let task_ids: Vec<String> = result["task_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        // Verify each task inherited overrides
        let tasks = state.tasks.read();
        for task_id in &task_ids {
            let task = tasks.get(task_id).expect("task should exist");
            assert_eq!(
                task.voice.as_deref(),
                Some("zh-CN-YunxiNeural"),
                "task {} should inherit default_voice",
                task_id
            );
            assert_eq!(
                task.model, "mimo-v3-tts",
                "task {} should inherit default_model",
                task_id
            );
            assert_eq!(
                task.context.as_deref(),
                Some("formal style"),
                "task {} should inherit default_context",
                task_id
            );
        }

        // Verify that tasks exist in state and have the right overrides
        // (group won't be in AppState.groups since submit doesn't call add_group)
        let group_id = result["group_id"].as_str().unwrap();
        assert_eq!(group_id.len(), 36, "group_id should be a UUID");
    }

    // ═══════════════════════════════════════════════════════════════════
    //  i) Submit without uploading → 404
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn submit_without_upload_returns_404() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/submit")
            .set_json(&serde_json::json!({
                "token": "nonexistent-token",
                "group_name": "Ghost",
            }))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404, "missing token should 404");

        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["error"].as_str().is_some(),
            "404 should include error message"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  j) Double submit → error 400
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn double_submit_returns_error() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/submit", web::post().to(submit)),
        )
        .await;

        // Upload
        let boundary = "double-boundary";
        let body = multipart_body(boundary, vec![("file", "double.txt", b"Single item\n")]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        let submit_body = serde_json::json!({
            "token": token,
            "group_name": "Double Group",
        });

        // First submit → OK
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/submit")
            .set_json(&submit_body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "first submit should succeed");

        // Second submit → 400 (already submitted)
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/submit")
            .set_json(&submit_body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "double submit should be rejected");

        let bytes = actix_test::read_body(resp).await;
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let error_msg = err["error"].as_str().unwrap_or("");
        assert!(
            error_msg.contains("submitted"),
            "error message should mention 'submitted', got: {}",
            error_msg
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  k) Extend session TTL
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn extend_ttl_updates_expiry() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/extend", web::post().to(extend_ttl)),
        )
        .await;

        // Upload
        let boundary = "ttl-boundary";
        let body = multipart_body(boundary, vec![("file", "ttl.txt", b"Extend my TTL\n")]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        // Record original expires_at
        let original = state.batch_imports.get_import(&token).unwrap();
        let original_expires = original.expires_at;

        // Extend TTL
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/extend")
            .set_json(&serde_json::json!({ "token": token }))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            result["status"].as_str(),
            Some("extended"),
            "response should indicate extension"
        );

        // Verify TTL was extended
        let extended = state.batch_imports.get_import(&token).unwrap();
        assert!(
            extended.expires_at > original_expires,
            "new expires_at should be later than original"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  l) Zip file upload with 3 .txt files
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn zip_upload_accepts_archive() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file)),
        )
        .await;

        // Create a zip with three text files
        let zip_data = create_zip(vec![
            ("file1.txt", "Content from file 1\n"),
            ("file2.txt", "Content from file 2\n"),
            ("file3.txt", "Content from file 3\n"),
        ]);

        let boundary = "zip-boundary";
        let body = multipart_body(boundary, vec![("file", "archive.zip", &zip_data)]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "zip upload should not crash");

        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["token"].as_str().is_some(), "zip upload should return a token");
        assert!(json["stats"].is_object(), "zip upload should return stats");
        assert!(
            json["stats"]["total_items"].as_u64().is_some(),
            "stats.total_items should be present"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  m) Invalid content → handler accepts gracefully
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn invalid_content_is_handled_gracefully() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file)),
        )
        .await;

        // Upload random binary bytes as a "zip" file
        let junk: Vec<u8> = (0..100).map(|i| (i % 256) as u8).collect();

        let boundary = "junk-boundary";
        let body = multipart_body(boundary, vec![("file", "corrupt.zip", &junk)]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        // Handler uses from_utf8_lossy + split by newlines,
        // so random binary still "succeeds" (doesn't crash).
        assert_eq!(
            resp.status(),
            200,
            "handler should accept any binary content without crashing"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Additional edge cases
    // ═══════════════════════════════════════════════════════════════════

    #[actix_web::test]
    async fn edit_item_with_invalid_token_returns_error() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/items/{index}", web::put().to(update_item)),
        )
        .await;

        let body = serde_json::json!({
            "token": "no-such-token",
            "voice": "test-voice",
        });
        let req = actix_test::TestRequest::put()
            .uri("/api/v1/batch/items/0")
            .set_json(&body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "invalid token should error");
    }

    #[actix_web::test]
    async fn extend_ttl_with_invalid_token_returns_404() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/extend", web::post().to(extend_ttl)),
        )
        .await;

        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/extend")
            .set_json(&serde_json::json!({ "token": "no-such-token" }))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404, "extend with invalid token should 404");
    }

    #[actix_web::test]
    async fn upload_json_lines_parses_fields() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file)),
        )
        .await;

        // Upload JSON-line format with mixed JSON/plain lines
        let content = r#"{"text":"Hello","voice":"v1","model":"m1","title":"T1"}
{"text":"World","voice":"v2","title":"T2"}
Just a plain line
"#;
        let boundary = "json-boundary";
        let body = multipart_body(boundary, vec![("file", "json.txt", content.as_bytes())]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        let import = state.batch_imports.get_import(&token).unwrap();
        assert_eq!(import.items.len(), 3, "should parse 3 items");

        // Item 0: JSON with all fields
        assert_eq!(import.items[0].text, "Hello");
        assert_eq!(import.items[0].voice.as_deref(), Some("v1"));
        assert_eq!(import.items[0].model.as_deref(), Some("m1"));
        assert_eq!(import.items[0].title.as_deref(), Some("T1"));

        // Item 1: JSON partial fields
        assert_eq!(import.items[1].text, "World");
        assert_eq!(import.items[1].voice.as_deref(), Some("v2"));
        assert!(import.items[1].model.is_none());

        // Item 2: plain text fallback
        assert_eq!(import.items[2].text, "Just a plain line");
        assert!(import.items[2].voice.is_none());
    }

    #[actix_web::test]
    async fn preview_default_pagination() {
        let state = create_state();
        let app = actix_test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/api/v1/batch/upload", web::post().to(upload_file))
                .route("/api/v1/batch/preview", web::get().to(get_preview)),
        )
        .await;

        // Upload 100 lines
        let content = (1..=100)
            .map(|i| format!("Long item {}\n", i))
            .collect::<String>();
        let boundary = "many-boundary";
        let body = multipart_body(boundary, vec![("file", "many.txt", content.as_bytes())]);
        let req = actix_test::TestRequest::post()
            .uri("/api/v1/batch/upload")
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let bytes = actix_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = json["token"].as_str().unwrap().to_string();

        // Preview with default pagination (per_page defaults to 50)
        let req = actix_test::TestRequest::get()
            .uri(&format!("/api/v1/batch/preview?token={}&page=0", token))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let bytes = actix_test::read_body(resp).await;
        let preview: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(preview["total"], 100);
        assert_eq!(preview["per_page"], 50, "default per_page should be 50");
        assert_eq!(preview["items"].as_array().unwrap().len(), 50);
        assert_eq!(preview["total_pages"], 2);
    }
}
