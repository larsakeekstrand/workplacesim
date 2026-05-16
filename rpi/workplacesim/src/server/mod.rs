//! Axum HTTP front-end.
//!
//! Mirrors `server/index.js` endpoint-for-endpoint including the SSE stream
//! and the two embedded frontend files. Task #3 of the Ethereal Thimble plan
//! adds the `/config` + `/api/config*` + `/api/status` surface that backs the
//! live-tuning web page.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::FromRef;
use axum::http::{header, Method};
use axum::routing::{get, post};
use axum::Router;
use parking_lot::RwLock;
use tower_http::cors::{Any, CorsLayer};

use crate::config::{persist::ConfigSource, SharedConfig};
use crate::state::State;

pub mod error;
pub mod mdns;
pub mod routes;
pub mod sse;
pub mod static_files;

pub type Shared = Arc<RwLock<State>>;

/// Compile-time build metadata surfaced on `/api/status`. Populated once at
/// boot via `BuildInfo::collect` — `version` is from Cargo, `git_sha` is the
/// `GIT_SHA` env at compile time (via a git pre-hook or `cross` wrapper), and
/// `features` is a snapshot of which renderer feature the binary was built
/// with.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct BuildInfo {
    pub version: &'static str,
    pub git_sha: Option<&'static str>,
    pub features: &'static str,
}

impl BuildInfo {
    /// Capture build metadata from `env!`/`option_env!`. Safe to call in any
    /// cfg combination — `features` is compile-time picked.
    pub fn collect() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("GIT_SHA"),
            features: feature_tag(),
        }
    }
}

/// Stringly enum for `BuildInfo.features`. Matches the mutually-exclusive
/// feature set enforced in `main.rs`.
const fn feature_tag() -> &'static str {
    #[cfg(all(feature = "desktop", not(feature = "fb")))]
    {
        "desktop"
    }
    #[cfg(all(feature = "fb", not(feature = "desktop")))]
    {
        "fb"
    }
    #[cfg(not(any(feature = "desktop", feature = "fb")))]
    {
        "server-only"
    }
    // The `all(desktop, fb)` combo is rejected by a compile_error! in main.rs,
    // so we don't need a branch for it.
}

/// Bundle of per-process handles that axum handlers can extract via
/// `FromRef`. Existing routes that used `State<Shared>` keep their signature
/// unchanged — the `FromRef<AppState> for Shared` impl below makes that work.
#[derive(Clone)]
pub struct AppState {
    pub state: Shared,
    pub config: SharedConfig,
    /// Wall-clock ms at server startup. `/api/status` reports uptime as
    /// `clock::now_ms() - started_at_ms`.
    pub started_at_ms: u64,
    pub build: BuildInfo,
    /// Absolute path the config is read from / written to. Captured once at
    /// startup so every request gets the same path — avoids re-resolving env
    /// and XDG on the hot path.
    pub config_path: PathBuf,
    /// Where the initial config came from. "loaded" / "missing-used-defaults"
    /// / "corrupt-used-defaults". Deliberately does NOT update when the user
    /// POSTs a new config — it's the "what we booted with" tag, not a live
    /// mirror of the file.
    pub config_source: ConfigSource,
    /// Live framebuffer/window metrics populated by the active renderer
    /// (Task #5). `None` on server-only builds and before the renderer
    /// finishes opening the fb/window; once populated, values update when
    /// the desktop window is recreated (config change) or on fb open.
    /// Read on every `/api/status` request via a short read-lock.
    pub fb_info: Arc<RwLock<Option<routes::FbInfo>>>,
}

impl FromRef<AppState> for Shared {
    fn from_ref(app: &AppState) -> Self {
        app.state.clone()
    }
}

impl FromRef<AppState> for SharedConfig {
    fn from_ref(app: &AppState) -> Self {
        app.config.clone()
    }
}

impl AppState {
    /// Convenience constructor for tests and the server-only bootstrap. The
    /// binary's three main() branches call this indirectly through
    /// `build_app_state` in `main.rs`; integration tests call it directly to
    /// get a consistent `AppState` without having to recreate every field.
    pub fn new(
        state: Shared,
        config: SharedConfig,
        config_path: PathBuf,
        config_source: ConfigSource,
    ) -> Self {
        Self {
            state,
            config,
            started_at_ms: crate::state::clock::now_ms(),
            build: BuildInfo::collect(),
            config_path,
            config_source,
            fb_info: Arc::new(RwLock::new(None)),
        }
    }

    /// Test helper that wires up an `AppState` with an ephemeral config path
    /// (useful when a test's config POSTs should land somewhere harmless).
    #[doc(hidden)]
    pub fn for_tests(state: Shared, config: SharedConfig) -> Self {
        Self::new(
            state,
            config,
            PathBuf::from("/dev/null"),
            ConfigSource::MissingUsedDefaults,
        )
    }
}

/// Build the axum router wired to the provided app state. Separate from
/// `run` so integration tests can `oneshot` requests without binding a port.
pub fn build_router(app: AppState) -> Router {
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
        // Task #3 additions — config UI surface.
        .route("/config", get(static_files::config_html))
        .route(
            "/api/config",
            get(routes::get_config).post(routes::post_config),
        )
        .route("/api/config/bounds", get(routes::get_config_bounds))
        .route("/api/config/reset", post(routes::reset_config))
        .route("/api/restart", post(routes::post_restart))
        .route("/api/status", get(routes::get_status))
        .with_state(app)
        .layer(cors)
}

/// Bind to `addr` and serve until the future returned by `shutdown` resolves.
/// Also registers mDNS for the bound port (best-effort; see `mdns` module).
pub async fn run(
    addr: SocketAddr,
    app: AppState,
    hostname: &str,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr().unwrap_or(addr);
    // Guard stays alive for the serve() lifetime; dropped on return so
    // unregister runs before the process exits cleanly via systemd.
    let _mdns = mdns::register(bound, hostname);
    let router = build_router(app);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
