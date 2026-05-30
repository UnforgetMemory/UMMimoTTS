//! Server-Sent Events endpoint.
//!
//! Exposes a long-lived GET /api/v2/events?channel=xxx endpoint that
//! streams DomainEvents to connected clients as SSE (text/event-stream).
//! The channel query parameter selects which event topic to subscribe to
//! (e.g. `batch:{id}`, `task:{id}`).

#![allow(dead_code)]

use crate::domain::events::DomainEvent;
use actix_web::{web, HttpResponse, Responder};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

use super::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/api/v2/events").route(web::get().to(events_stream)));
}

async fn events_stream(
    state: web::Data<AppState>,
    query: web::Query<ChannelQuery>,
) -> impl Responder {
    let mut rx = state.sse_bus.subscribe(&query.channel);

    // Bridge: spawn a task that reads from broadcast and forwards to mpsc.
    // This avoids the self-referential struct problem and ensures the
    // broadcast::Receiver's `recv()` future properly registers the waker.
    let (tx, mpsc_rx) = mpsc::channel::<DomainEvent>(256);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if tx.send(event).await.is_err() {
                        break; // Client disconnected
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Skip lagged events, continue
                    continue;
                }
            }
        }
    });

    // Convert mpsc receiver into an SSE stream
    let stream = ReceiverStream::new(mpsc_rx).map(
        |event: DomainEvent| {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(format!(
                "data: {data}\n\n"
            )))
        },
    );

    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}

#[derive(serde::Deserialize)]
pub struct ChannelQuery {
    pub channel: String,
}
