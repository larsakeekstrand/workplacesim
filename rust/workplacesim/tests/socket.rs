//! Binds a real TCP listener on a random port and drives the server through
//! reqwest. Proves the network path compiles and serves, in addition to the
//! `oneshot` router coverage in `http.rs`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde_json::json;
use workplacesim::config::{self, Config};
use workplacesim::server::AppState;
use workplacesim::{server, state};

#[tokio::test]
async fn real_socket_roundtrip() {
    let cfg = config::shared(Config::default());
    let (shared, _rx) = state::new_state(cfg.clone());

    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
    let addr = listener.local_addr().unwrap();

    // Build the router exactly as `server::run` does, but use a listener we
    // already bound so the test knows the port.
    let app = server::build_router(AppState::for_tests(shared.clone(), cfg));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base = format!("http://{addr}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    // Start a subagent.
    let resp = client
        .post(format!("{base}/hooks/subagent-start"))
        .json(&json!({
            "agent_id": "a1",
            "agent_type": "claude",
            "session_id": "sess",
            "user": "daisy"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 204);

    // Fetch agents and verify the shape.
    let resp = client
        .get(format!("{base}/api/agents"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let agents = body["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["agent_id"].as_str(), Some("a1"));

    server.abort();
}
