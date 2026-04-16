//! Handlers for the six hook POST endpoints + `/api/agents`. Each handler
//! takes the write lock only long enough to mutate state, then drops it; the
//! state methods themselves own event emission.

use axum::extract::State as AxumState;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use super::Shared;
use crate::state::{
    clock, BufferDescription, Lifecycle, Pretool, StartAgent, StopAgent, ToolEvent, VisitRoom,
};

pub async fn pretool(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<Pretool>,
) -> StatusCode {
    // Gate exactly as `server/index.js`: only buffer when the PreToolUse is
    // for the `Agent` tool. Other tool invocations reach this route but are
    // no-ops. `buffer_description` itself drops payloads with no
    // `subagent_type`, so we don't duplicate that check here.
    if body.tool_name.as_deref() == Some("Agent") {
        let now = clock::now_ms();
        let mut guard = state.write().await;
        guard.buffer_description(
            BufferDescription {
                session_id: body.session_id,
                subagent_type: body.tool_input.subagent_type,
                description: body.tool_input.description,
                tool_use_id: body.tool_use_id,
            },
            now,
        );
    }
    StatusCode::NO_CONTENT
}

pub async fn subagent_start(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<StartAgent>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write().await;
    guard.start_agent(body, now);
    StatusCode::NO_CONTENT
}

pub async fn subagent_stop(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<StopAgent>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write().await;
    guard.stop_agent(body, now);
    StatusCode::NO_CONTENT
}

pub async fn lab_visit(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<VisitRoom>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write().await;
    guard.visit_room(body, now);
    StatusCode::NO_CONTENT
}

pub async fn tool_event(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<ToolEvent>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write().await;
    guard.tool_event(body, now);
    StatusCode::NO_CONTENT
}

pub async fn lifecycle(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<Lifecycle>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write().await;
    guard.handle_lifecycle(body, now);
    StatusCode::NO_CONTENT
}

pub async fn list_agents(AxumState(state): AxumState<Shared>) -> Json<Value> {
    let guard = state.read().await;
    Json(json!({ "agents": guard.list_active() }))
}
