use crate::state::app_state::{AppState, TaskEvent};
use actix_web::{web, HttpResponse, Responder};
use futures::stream::Stream;
use std::time::Duration;

/// SSE endpoint for individual task events
pub async fn sse_task_events(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let task_id = path.into_inner();
    let rx = data.subscribe_events(task_id.clone());

    let stream = async_stream::stream! {
        // 发送初始连接消息
        yield Ok::<_, actix_web::Error>(
            web::Bytes::from(format!("event: connected\ndata: {{\"task_id\":\"{}\"}}\n\n", task_id))
        );

        // 监听事件
        while let Ok(event) = rx.recv_async().await {
            let data_str = match &event {
                TaskEvent::StatusChanged { task_id, status, progress } => {
                    let data = serde_json::json!({
                        "task_id": task_id,
                        "event_type": "status_changed",
                        "status": status,
                        "progress": progress
                    });
                    format!("event: status_changed\ndata: {}\n\n", data)
                }
                TaskEvent::Completed { task_id } => {
                    let data = serde_json::json!({
                        "task_id": task_id,
                        "event_type": "completed"
                    });
                    format!("event: completed\ndata: {}\n\n", data)
                }
                TaskEvent::Failed { task_id, error } => {
                    let data = serde_json::json!({
                        "task_id": task_id,
                        "event_type": "failed",
                        "error": error
                    });
                    format!("event: failed\ndata: {}\n\n", data)
                }
            };

            yield Ok(web::Bytes::from(data_str));
        }
    };

    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}

/// SSE endpoint for batch group events
/// Subscribes to all tasks in the group and forwards events with group context
pub async fn sse_group_events(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let group_id = path.into_inner();

    // Get group and its task IDs
    let task_ids = {
        let groups = data.groups.read();
        match groups.get(&group_id) {
            Some(group) => group.task_ids.clone(),
            None => {
                return HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Group not found",
                    "group_id": group_id
                }))
            }
        }
    };

    // Subscribe to all tasks in the group
    let receivers: Vec<_> = task_ids
        .iter()
        .map(|task_id| (task_id.clone(), data.subscribe_events(task_id.clone())))
        .collect();

    let app_data = data.clone();
    let gid = group_id.clone();

    let stream = async_stream::stream! {
        // Send initial connection message
        yield Ok::<_, actix_web::Error>(web::Bytes::from(format!(
            "event: connected\ndata: {{\"group_id\":\"{}\"}}\n\n",
            group_id
        )));

        // Merge events from all tasks
        let mut receivers = receivers;
        loop {
            // Try to receive from any task
            let mut received = false;

            for (task_id, rx) in &receivers {
                match rx.try_recv() {
                    Ok(event) => {
                        received = true;
                        // Fetch group stats for every event
                        let group_stats = app_data.get_group(&gid).map(|g| {
                            let stats = serde_json::json!({
                                "completed_tasks": g.completed_tasks,
                                "failed_tasks": g.failed_tasks,
                                "total_tasks": g.total_tasks
                            });
                            (g.completed_tasks, g.failed_tasks, g.total_tasks, g.status.clone(), stats)
                        });

                        let data_str = match &event {
                            TaskEvent::StatusChanged { task_id, status, progress } => {
                                let mut data = serde_json::json!({
                                    "group_id": gid,
                                    "task_id": task_id,
                                    "event_type": "progress",
                                    "status": status,
                                    "progress": progress
                                });
                                if let Some(ref stats) = group_stats {
                                    data["completed_tasks"] = stats.0.into();
                                    data["failed_tasks"] = stats.1.into();
                                    data["total_tasks"] = stats.2.into();
                                    // Also include the group status so frontend can update it
                                    data["group_status"] = serde_json::json!(stats.3);
                                }
                                let event_data = serde_json::to_string(&data).unwrap_or_default();
                                format!("event: progress\ndata: {}\n\n", event_data)
                            }
                            TaskEvent::Completed { task_id } => {
                                let mut data = serde_json::json!({
                                    "group_id": gid,
                                    "task_id": task_id,
                                    "event_type": "task_completed"
                                });
                                if let Some(ref stats) = group_stats {
                                    data["completed_tasks"] = stats.0.into();
                                    data["failed_tasks"] = stats.1.into();
                                    data["total_tasks"] = stats.2.into();
                                    data["group_status"] = serde_json::json!(stats.3);
                                }
                                let event_data = serde_json::to_string(&data).unwrap_or_default();
                                format!("event: task_completed\ndata: {}\n\n", event_data)
                            }
                            TaskEvent::Failed { task_id, error } => {
                                let mut data = serde_json::json!({
                                    "group_id": gid,
                                    "task_id": task_id,
                                    "event_type": "task_failed",
                                    "error": error
                                });
                                if let Some(ref stats) = group_stats {
                                    data["completed_tasks"] = stats.0.into();
                                    data["failed_tasks"] = stats.1.into();
                                    data["total_tasks"] = stats.2.into();
                                    data["group_status"] = serde_json::json!(stats.3);
                                }
                                let event_data = serde_json::to_string(&data).unwrap_or_default();
                                format!("event: task_failed\ndata: {}\n\n", event_data)
                            }
                        };
                        yield Ok(web::Bytes::from(data_str));

                        // Check if group is complete
                        if let Some((completed, failed, total, _, _)) = group_stats {
                            if completed + failed >= total {
                                let event_type = if failed > 0 { "group_failed" } else { "group_completed" };
                                let data = serde_json::json!({
                                    "group_id": gid,
                                    "event_type": event_type,
                                    "completed_tasks": completed,
                                    "failed_tasks": failed,
                                    "total_tasks": total
                                });
                                yield Ok(web::Bytes::from(format!("event: {}\ndata: {}\n\n", event_type, data)));
                                return;
                            }
                        }
                    }
                    Err(flume::TryRecvError::Empty) => {}
                    Err(flume::TryRecvError::Disconnected) => {}
                }
            }

            if !received {
                // Small delay to avoid busy-waiting
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    };

    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}
