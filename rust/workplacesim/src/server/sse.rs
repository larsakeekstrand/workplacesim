//! `GET /events` — Server-Sent Events for the Phaser frontend.
//!
//! Wire format and semantics match `server/index.js`:
//! - `Content-Type: text/event-stream` (axum sets this automatically).
//! - `Cache-Control: no-cache, no-transform` (axum sets `no-cache`; we
//!   override to include `no-transform`, matching the Node server so
//!   intermediate proxies can't gzip the stream).
//! - `X-Accel-Buffering: no` so nginx and friends don't buffer the stream.
//! - Initial `data: {"type":"snapshot",...}\n\n` synthesized at subscribe
//!   time, then `broadcast::Receiver<Event>` events fanned out as JSON.
//! - Keepalive `: ping\n\n` every 25 s (SSE comment, ignored by EventSource).
//! - `RecvError::Lagged` is rare (broadcast capacity is 256) but, when it
//!   happens, we log and push a fresh snapshot so the client can resync.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State as AxumState;
use axum::http::{header, HeaderValue};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

use super::Shared;
use crate::state::Event;

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(25);

pub async fn events(AxumState(state): AxumState<Shared>) -> Response {
    // Build the initial snapshot and a fresh subscriber under a single read
    // lock so nothing can interleave between snapshot capture and subscription.
    let (snapshot, rx) = {
        let guard = state.read();
        (guard.snapshot_event(), guard.subscribe_events())
    };

    let stream = event_stream(snapshot, rx, state.clone());
    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(KEEPALIVE_INTERVAL)
            // Matches the Node server's ": ping\n\n" payload byte-for-byte.
            .text(" ping"),
    );

    let mut response = sse.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert("X-Accel-Buffering", HeaderValue::from_static("no"));
    response
}

/// Build the outbound SSE event stream: one synthesized snapshot, then the
/// broadcast channel mapped through JSON serialization. Lagged subscribers
/// get a fresh snapshot so the browser resyncs rather than desynchronizing
/// silently.
fn event_stream(
    snapshot: Event,
    rx: tokio::sync::broadcast::Receiver<Event>,
    state: Shared,
) -> impl Stream<Item = Result<SseEvent, Infallible>> + Send + 'static {
    let initial = stream::once(async move { to_sse(&snapshot) });

    let live = BroadcastStream::new(rx).filter_map(move |item| {
        let state = state.clone();
        async move {
            match item {
                Ok(ev) => Some(to_sse(&ev)),
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    tracing::warn!(missed = n, "SSE subscriber lagged; sending fresh snapshot");
                    let snap = state.read().snapshot_event();
                    Some(to_sse(&snap))
                }
            }
        }
    });

    initial.chain(live)
}

fn to_sse(event: &Event) -> Result<SseEvent, Infallible> {
    // serde_json::to_string on our `Event` enum cannot fail in practice — all
    // fields are owned Strings / numbers with no custom Serialize impls. On
    // the off chance it does, fall back to a minimal error envelope so the
    // stream stays open.
    let payload = serde_json::to_string(event)
        .unwrap_or_else(|_| r#"{"type":"error","message":"serialize-failed"}"#.to_string());
    Ok(SseEvent::default().data(payload))
}
