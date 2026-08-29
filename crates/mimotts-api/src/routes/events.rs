//! GET /api/v3/events — SSE stream (?channel=...&token=...).
//!
//! EventSource cannot set an Authorization header, so the token may also be
//! passed as a query parameter (hash-checked, never stored in logs by us).
//! Header bearer is accepted as an alternative.
//!
//! Heartbeat comment every 15s keeps proxies from killing idle streams.

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/events", web::get().to(stream));
}

#[derive(Deserialize)]
struct ChannelQuery {
    channel: String,
    token: Option<String>,
}

async fn stream(
    req: actix_web::HttpRequest,
    state: web::Data<AppState>,
    q: web::Query<ChannelQuery>,
) -> HttpResponse {
    // Auth: scoped credential / query API token / bearer header.
    let scope = format!("events:{}", q.channel);
    if !crate::auth::scoped_or_bearer_ok(&req, &state.engine, q.token.as_deref(), &scope) {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "missing or invalid token (scoped/API token via query, or Authorization header)",
            "code": "UNAUTHORIZED",
        }));
    }

    let mut rx = state.engine.bus.subscribe(&q.channel);
    let (tx, rx_out) = tokio::sync::mpsc::channel::<String>(256);
    let channel = q.channel.clone();

    tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tokio::select! {
                event = rx.recv() => match event {
                    Ok(ev) => {
                        let data = serde_json::to_string(&ev).unwrap_or_default();
                        if tx.send(format!("data: {data}\n\n")).await.is_err() {
                            break; // client gone
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("sse {channel} lagged by {n}");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = heartbeat.tick() => {
                    if tx.send(": keepalive\n\n".to_string()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx_out).map(|s| {
        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(s))
    });
    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}
