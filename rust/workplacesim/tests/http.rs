//! Integration tests that exercise the axum router via `oneshot`, without
//! binding a TCP port. Covers the checklist in the step-2 brief.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::Value;
use tokio::sync::broadcast::error::TryRecvError;
use tower::ServiceExt;
use workplacesim::{server, state, Event};

fn json_post(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<Event> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(ev) => out.push(ev),
            Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
    out
}

#[tokio::test]
async fn pretool_agent_buffers_description_consumed_by_start() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    // PreToolUse(Agent) with tool_input → buffers.
    let req = json_post(
        "/hooks/pretool",
        r#"{
            "session_id": "sess",
            "tool_name": "Agent",
            "tool_use_id": "tu-1",
            "tool_input": {
                "subagent_type": "verifier",
                "description": "run the verifier"
            }
        }"#,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(drain(&mut rx).is_empty(), "buffering does not emit");
    {
        let guard = state.read().await;
        assert_eq!(guard.list_active().len(), 0);
    }

    // Now a subagent-start with the tu-1 id and empty description pulls it.
    let req = json_post(
        "/hooks/subagent-start",
        r#"{
            "agent_id": "tu-1",
            "session_id": "sess",
            "agent_type": "verifier"
        }"#,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Start { agent } => {
            assert_eq!(agent.agent_id, "tu-1");
            assert_eq!(agent.description, "run the verifier");
        }
        other => panic!("expected Start, got {other:?}"),
    }
}

#[tokio::test]
async fn pretool_non_agent_tool_is_noop() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    let req = json_post(
        "/hooks/pretool",
        r#"{
            "session_id": "sess",
            "tool_name": "Bash",
            "tool_input": {"command": "ls"}
        }"#,
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(drain(&mut rx).is_empty());

    let guard = state.read().await;
    assert_eq!(guard.list_active().len(), 0);
    // No pending description buffered either — buffer_description requires
    // subagent_type, so a Bash payload with no subagent_type wouldn't buffer
    // even if we let it through. But the gate also blocks it.
}

#[tokio::test]
async fn subagent_start_roundtrip_and_api_agents_shape() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    let req = json_post(
        "/hooks/subagent-start",
        r#"{"agent_id":"a1","agent_type":"claude","user":"daisy","session_id":"sess"}"#,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Event::Start { agent } if agent.agent_id == "a1"));

    // /api/agents → {"agents":[{...}]}
    let req = Request::builder()
        .method("GET")
        .uri("/api/agents")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let agents = v.get("agents").expect("top-level agents key").as_array().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["agent_id"].as_str(), Some("a1"));
    assert_eq!(agents[0]["user"].as_str(), Some("daisy"));
}

#[tokio::test]
async fn subagent_stop_direct_removes_from_list() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    let req = json_post(
        "/hooks/subagent-start",
        r#"{"agent_id":"a1","agent_type":"claude","session_id":"sess"}"#,
    );
    app.clone().oneshot(req).await.unwrap();
    let _ = drain(&mut rx);

    let req = json_post("/hooks/subagent-stop", r#"{"agent_id":"a1"}"#);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Event::Stop { agent_id } if agent_id == "a1"));

    // /api/agents excludes finished records.
    let req = Request::builder()
        .method("GET")
        .uri("/api/agents")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["agents"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn subagent_stop_fifo_fallback() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    // Two subagents same (session_id, agent_type). Oldest is tu-1.
    for id in ["tu-1", "tu-2"] {
        let body = format!(
            r#"{{"agent_id":"{id}","agent_type":"verifier","session_id":"sess"}}"#
        );
        app.clone().oneshot(json_post("/hooks/subagent-start", &body)).await.unwrap();
    }
    let _ = drain(&mut rx);

    // SubagentStop arrives with an agent_id that doesn't match either record.
    let req = json_post(
        "/hooks/subagent-stop",
        r#"{"agent_id":"different","session_id":"sess","agent_type":"verifier"}"#,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Event::Stop { agent_id } if agent_id == "tu-1"));

    // tu-2 still listed.
    let guard = state.read().await;
    let active = guard.list_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].agent_id, "tu-2");
}

#[tokio::test]
async fn lab_visit_emits_visit_with_until() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    app.clone()
        .oneshot(json_post(
            "/hooks/subagent-start",
            r#"{"agent_id":"a1","agent_type":"claude","session_id":"sess"}"#,
        ))
        .await
        .unwrap();
    let _ = drain(&mut rx);

    let req = json_post(
        "/hooks/lab-visit",
        r#"{"agent_id":"a1","room":"test","ttl_ms":5000}"#,
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Visit { agent_id, room, until } => {
            assert_eq!(agent_id, "a1");
            assert_eq!(room.as_deref(), Some("test"));
            assert!(until.is_some());
        }
        other => panic!("expected Visit, got {other:?}"),
    }
}

#[tokio::test]
async fn tool_event_known_agent_emits_tool_unknown_is_204_and_silent() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    app.clone()
        .oneshot(json_post(
            "/hooks/subagent-start",
            r#"{"agent_id":"a1","agent_type":"claude","session_id":"sess"}"#,
        ))
        .await
        .unwrap();
    let _ = drain(&mut rx);

    // Known agent → emits Tool.
    let resp = app
        .clone()
        .oneshot(json_post(
            "/hooks/tool-event",
            r#"{"agent_id":"a1","tool_name":"Read"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Event::Tool { tool_name, .. } if tool_name == "Read"));

    // Unknown agent → still 204, no emit.
    let resp = app
        .oneshot(json_post(
            "/hooks/tool-event",
            r#"{"agent_id":"ghost","tool_name":"Read"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn lifecycle_prompt_truncates_text() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    app.clone()
        .oneshot(json_post(
            "/hooks/subagent-start",
            r#"{"agent_id":"a1","agent_type":"claude","session_id":"sess"}"#,
        ))
        .await
        .unwrap();
    let _ = drain(&mut rx);

    let long_text = "x".repeat(200);
    let body = serde_json::json!({
        "kind": "prompt",
        "agent_id": "a1",
        "text": long_text
    })
    .to_string();

    let resp = app
        .oneshot(json_post("/hooks/lifecycle", &body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Prompt { text, agent_id } => {
            assert_eq!(text.len(), 80);
            assert_eq!(agent_id, "a1");
        }
        other => panic!("expected Prompt, got {other:?}"),
    }
}

#[tokio::test]
async fn lifecycle_tool_error_stores_current_error_and_emits() {
    let (state, mut rx) = state::new_state();
    let app = server::build_router(state.clone());

    app.clone()
        .oneshot(json_post(
            "/hooks/subagent-start",
            r#"{"agent_id":"a1","agent_type":"claude","session_id":"sess"}"#,
        ))
        .await
        .unwrap();
    let _ = drain(&mut rx);

    let long = "e".repeat(200);
    let body = serde_json::json!({
        "kind": "tool-error",
        "agent_id": "a1",
        "tool_name": "Bash",
        "message": long
    })
    .to_string();

    let resp = app
        .oneshot(json_post("/hooks/lifecycle", &body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ToolError { message, tool_name, agent_id } => {
            assert_eq!(message.len(), 80);
            assert_eq!(tool_name, "Bash");
            assert_eq!(agent_id, "a1");
        }
        other => panic!("expected ToolError, got {other:?}"),
    }

    // current_error stashed on the record too.
    let guard = state.read().await;
    let agent = guard.list_active().into_iter().next().unwrap();
    let err = agent.current_error.unwrap();
    assert_eq!(err.tool_name, "Bash");
    assert_eq!(err.message.len(), 80);
}

#[tokio::test]
async fn cors_preflight_returns_allow_origin_star() {
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/hooks/subagent-start")
        .header("origin", "http://example.com")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // CorsLayer returns 200 for preflight (not 204); the important bit is the
    // Access-Control-Allow-Origin header being present.
    assert!(resp.status().is_success());
    let origin = resp
        .headers()
        .get("access-control-allow-origin")
        .expect("preflight missing Allow-Origin");
    assert_eq!(origin.to_str().unwrap(), "*");
}

#[tokio::test]
async fn malformed_json_body_returns_400() {
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    let req = json_post("/hooks/subagent-start", r#"{ not json"#);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_agents_empty_shape() {
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    // Exact shape: {"agents":[]}
    assert!(v.is_object());
    assert_eq!(v.as_object().unwrap().len(), 1);
    assert!(v["agents"].is_array());
    assert_eq!(v["agents"].as_array().unwrap().len(), 0);
}
