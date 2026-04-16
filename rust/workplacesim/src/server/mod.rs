//! Axum HTTP front-end.
//!
//! Mirrors `server/index.js` endpoint-for-endpoint including the SSE stream
//! and the two embedded frontend files.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::{header, Method};
use axum::routing::{get, post};
use axum::Router;
use parking_lot::RwLock;
use tower_http::cors::{Any, CorsLayer};

use crate::state::State;

pub mod error;
pub mod routes;
pub mod sse;
pub mod static_files;

pub type Shared = Arc<RwLock<State>>;

/// Build the axum router wired to the provided shared state. Separate from
/// `run` so integration tests can `oneshot` requests without binding a port.
pub fn build_router(state: Shared) -> Router {
    // Matches the ad-hoc Node CORS middleware: origin *, methods GET/POST/OPTIONS,
    // content-type header. Preflights handled by CorsLayer itself.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE]);

    Router::new()
        .route("/hooks/pretool", post(routes::pretool))
        .route("/hooks/subagent-start", post(routes::subagent_start))
        .route("/hooks/subagent-stop", post(routes::subagent_stop))
        .route("/hooks/lab-visit", post(routes::lab_visit))
        .route("/hooks/tool-event", post(routes::tool_event))
        .route("/hooks/lifecycle", post(routes::lifecycle))
        .route("/api/agents", get(routes::list_agents))
        .route("/events", get(sse::events))
        .route("/", get(static_files::index))
        .route("/main.js", get(static_files::main_js))
        .with_state(state)
        .layer(cors)
}

/// Bind to `addr` and serve until the future returned by `shutdown` resolves.
pub async fn run(
    addr: SocketAddr,
    state: Shared,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let router = build_router(state);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
