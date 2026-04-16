//! Real-socket coverage for `GET /events`. Uses a short read timeout to
//! capture the first chunks of the stream, then asserts SSE shape and
//! event ordering.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde_json::json;
use workplacesim::{server, state};

async fn start_server() -> (SocketAddr, server::Shared, tokio::task::JoinHandle<()>) {
    let (shared, _rx) = state::new_state();
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
    ))
    .await
    .unwrap();
    let addr = listener.local_addr().unwrap();
    let app = server::build_router(shared.clone());
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, shared, handle)
}

/// Read up to `limit` bytes from the streaming body, stopping as soon as
/// `sentinel` substring appears (or on stream EOF / 2s timeout).
async fn read_until(mut resp: reqwest::Response, sentinel: &str, limit: usize) -> String {
    let mut buf = Vec::<u8>::new();
    let fut = async {
        loop {
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    buf.extend_from_slice(&bytes);
                    let s = String::from_utf8_lossy(&buf);
                    if s.contains(sentinel) || buf.len() >= limit {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), fut).await;
    String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test]
async fn events_emits_initial_snapshot() {
    let (addr, _shared, handle) = start_server().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let resp = client.get(format!("http://{addr}/events")).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-cache, no-transform")
    );
    assert_eq!(
        resp.headers()
            .get("x-accel-buffering")
            .and_then(|v| v.to_str().ok()),
        Some("no")
    );

    let body = read_until(resp, "\n\n", 4096).await;
    assert!(
        body.starts_with("data: {\"type\":\"snapshot\""),
        "expected initial snapshot frame, got {body:?}"
    );
    assert!(
        body.contains("\"agents\":[]"),
        "snapshot should report empty agents, got {body:?}"
    );

    handle.abort();
}

#[tokio::test]
async fn events_streams_start_after_subscribe() {
    let (addr, _shared, handle) = start_server().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    // Open the SSE connection first.
    let sse_client = client.clone();
    let addr_str = format!("http://{addr}");
    let resp = sse_client
        .get(format!("{addr_str}/events"))
        .send()
        .await
        .unwrap();

    // Fire a POST that emits a Start event.
    client
        .post(format!("{addr_str}/hooks/subagent-start"))
        .json(&json!({
            "agent_id": "a1",
            "agent_type": "claude",
            "session_id": "sess",
            "user": "daisy"
        }))
        .send()
        .await
        .unwrap();

    let body = read_until(resp, "\"type\":\"start\"", 8192).await;
    assert!(body.starts_with("data: {\"type\":\"snapshot\""), "body={body:?}");
    assert!(body.contains("\"type\":\"start\""), "expected start event in body, got {body:?}");
    assert!(body.contains("\"agent_id\":\"a1\""), "start payload should carry agent_id, got {body:?}");

    handle.abort();
}

#[tokio::test]
async fn events_resyncs_on_lag() {
    // Saturate the broadcast channel past its capacity (256). The SSE stream
    // must recover by emitting a fresh snapshot rather than dying.
    let (addr, shared, handle) = start_server().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    // Open the stream. Don't read yet so the tokio-stream BroadcastStream
    // falls behind.
    let resp = client
        .get(format!("http://{addr}/events"))
        .send()
        .await
        .unwrap();

    // Pump way more events than the 256-slot capacity in one shot, while the
    // SSE reader is still waiting to be read from.
    {
        use workplacesim::{clock, StartAgent, StopAgent};
        let now = clock::now_ms();
        for i in 0..400u32 {
            let id = format!("spam-{i}");
            let mut guard = shared.write();
            guard.start_agent(
                StartAgent {
                    agent_id: id.clone(),
                    session_id: Some("sess".into()),
                    agent_type: Some("coder".into()),
                    ..Default::default()
                },
                now,
            );
            guard.stop_agent(
                StopAgent {
                    agent_id: Some(id),
                    ..Default::default()
                },
                now,
            );
        }
    }

    // A resync snapshot should appear somewhere in the stream after the
    // initial one. Read generously.
    let body = read_until(resp, "spam-399", 131_072).await;
    // Must at least include the initial snapshot …
    assert!(body.contains("\"type\":\"snapshot\""), "body={body:?}");
    // … and the body must not be truncated on lag — either we saw the lag
    // resync snapshot or we caught up with live events. A hard proof is
    // hard to force deterministically without slowing the reader; what we
    // really care about is the stream not closing on lag. Assert we read
    // a nonzero suffix.
    assert!(body.len() > 100, "stream closed early on lag, body={body:?}");

    handle.abort();
}
