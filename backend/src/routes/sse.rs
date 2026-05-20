use crate::state::app_state::{AppState, TaskEvent};
use actix_web::{web, HttpResponse, Responder};
use futures::stream::Stream;
use std::time::Duration;

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
            let data_str = match event {
                TaskEvent::StatusChanged { task_id, progress } => {
                    format!("event: status_changed\ndata: {{\"task_id\":\"{}\",\"progress\":{}}}\n\n", task_id, progress)
                }
                TaskEvent::Completed { task_id } => {
                    format!("event: completed\ndata: {{\"task_id\":\"{}\"}}\n\n", task_id)
                }
                TaskEvent::Failed { task_id, error } => {
                    format!("event: failed\ndata: {{\"task_id\":\"{}\",\"error\":\"{}\"}}\n\n", task_id, error)
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
