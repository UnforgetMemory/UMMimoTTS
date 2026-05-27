//! Server-Sent Events endpoint.
//!
//! Exposes a long-lived GET /api/v2/events?channel=xxx endpoint that
//! streams DomainEvents to connected clients as SSE (text/event-stream).
//! The channel query parameter selects which event topic to subscribe to
//! (e.g. `batch:{id}`, `task:{id}`).

#![allow(dead_code)]

use crate::domain::events::DomainEvent;
use actix_web::{web, HttpResponse, Responder};
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;

use super::AppState;

/// SSE event stream wrapper around a broadcast receiver.
struct SseStream {
    rx: broadcast::Receiver<DomainEvent>,
}

impl Stream for SseStream {
    type Item = Result<actix_web::web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.try_recv() {
            Ok(event) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                let msg = format!("data: {data}\n\n");
                Poll::Ready(Some(Ok(actix_web::web::Bytes::from(msg))))
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // Register waker for future notifications
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(broadcast::error::TryRecvError::Closed) => Poll::Ready(None),
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                // Skip lagged events
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2")
            .route("/events", web::get().to(events_stream)),
    );
}

async fn events_stream(
    state: web::Data<AppState>,
    query: web::Query<ChannelQuery>,
) -> impl Responder {
    let rx = state.sse_bus.subscribe(&query.channel);
    let stream = SseStream { rx };
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
