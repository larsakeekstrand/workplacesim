use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use tracing_subscriber::EnvFilter;
use workplacesim::config::{self, persist::ConfigSource, SharedConfig};
use workplacesim::server::{AppState, BuildInfo};

// Guard against an accidental combination that the build system allows at the
// Cargo level but that produces two renderers in the main() branches below.
#[cfg(all(feature = "fb", feature = "desktop"))]
compile_error!(
    "features `fb` and `desktop` are mutually exclusive. Pick one:\n  \
     - desktop: cargo run --features desktop --no-default-features\n  \
     - fb:      cross build --target arm-unknown-linux-gnueabihf --features fb --no-default-features"
);

#[cfg_attr(all(feature = "fb", not(target_os = "linux")), allow(dead_code))]
fn bootstrap_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
}

/// Parse `--demo N` from argv. Returns `N` if the flag is present, else `None`.
/// Minimal on purpose — no clap dependency for a single-flag CLI.
#[cfg(feature = "desktop")]
fn parse_demo_count() -> Option<usize> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--demo" {
            return args.next().and_then(|s| s.parse().ok());
        }
        if let Some(val) = a.strip_prefix("--demo=") {
            return val.parse().ok();
        }
    }
    None
}

#[cfg(feature = "desktop")]
fn seed_demo_agents(state: &workplacesim::server::Shared, n: usize) {
    use workplacesim::state::{clock, StartAgent};
    let now = clock::now_ms();
    let profiles: &[(&str, &str, &str, &str)] = &[
        ("alice", "coder", "default", "edit the auth module"),
        ("bob", "verifier", "default", "run golden-frame tests"),
        ("carol", "planner", "plan", "draft the migration plan"),
        ("dave", "coder", "default", "refactor the router"),
        ("eve", "reviewer", "default", "lint the state module"),
        ("frank", "coder", "plan", "sketch the fb backend"),
    ];
    // Alternate two session_ids so the chest labels demo the per-session
    // assignment contract (same char across sims that share a session,
    // distinct char across concurrent sessions).
    for i in 0..n {
        let (user, ty, mode, desc) = profiles[i % profiles.len()];
        let id = format!("demo-{i}");
        let sid = format!("demo-session-{}", i % 2);
        let started_at = now + (i as u64) * 2_000;
        let mut s = state.write();
        s.start_agent(
            StartAgent {
                agent_id: id,
                session_id: Some(sid),
                agent_type: Some(ty.into()),
                description: Some(desc.into()),
                user: Some(user.into()),
                host: Some("demo".into()),
                cwd: Some("/tmp".into()),
                permission_mode: Some(mode.into()),
            },
            started_at,
        );
    }
}

/// Load the runtime config from disk (or defaults), log where it came from,
/// and wrap it in the shared handle used by state + renderer + server. Kept
/// separate from each `main()` branch so the three cfg-gated mains don't
/// duplicate the logging + Arc construction.
///
/// Returns `(shared, path, source)` so `AppState` can stash the path (for
/// persisted POSTs) and the source tag (for `/api/status`). Source deliberately
/// reflects what we loaded at startup; subsequent POSTs do not update it.
#[cfg_attr(all(feature = "fb", not(target_os = "linux")), allow(dead_code))]
fn load_shared_config() -> (SharedConfig, PathBuf, ConfigSource) {
    use config::persist::{load_or_default, resolve_path};

    let path = resolve_path();
    let (cfg, source) = load_or_default(&path);
    match source {
        ConfigSource::Loaded => {
            tracing::info!("workplacesim: loaded config from {}", path.display());
        }
        ConfigSource::MissingUsedDefaults => {
            tracing::info!(
                "workplacesim: no config at {}; using defaults",
                path.display()
            );
        }
        ConfigSource::CorruptUsedDefaults => {
            tracing::warn!(
                "workplacesim: corrupt config at {}; using defaults (fix-up requires /api/config POST)",
                path.display()
            );
        }
    }
    (config::shared(cfg), path, source)
}

/// Assemble the `AppState` shared by every axum handler. Done once per process
/// so all three `main()` branches construct the bundle identically.
#[cfg_attr(all(feature = "fb", not(target_os = "linux")), allow(dead_code))]
fn build_app_state(
    state: workplacesim::server::Shared,
    shared_config: SharedConfig,
    config_path: PathBuf,
    config_source: ConfigSource,
) -> AppState {
    AppState {
        state,
        config: shared_config,
        started_at_ms: workplacesim::state::clock::now_ms(),
        build: BuildInfo::collect(),
        config_path,
        config_source,
        // Populated by the active renderer (desktop/fb) once it opens its
        // surface. Server-only builds leave it `None` forever.
        fb_info: std::sync::Arc::new(parking_lot::RwLock::new(None)),
    }
}

#[cfg_attr(all(feature = "fb", not(target_os = "linux")), allow(dead_code))]
fn bind_addr() -> SocketAddr {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4317);
    let host: IpAddr = std::env::var("HOST")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    SocketAddr::new(host, port)
}

/// Resolve the hostname used for mDNS service instance names. Matches what
/// `avahi-daemon`'s `%h` expansion would have picked on the Pi: HOSTNAME env
/// var wins if set (handy for container / systemd overrides), then the kernel
/// hostname via nix, then a sane literal fallback.
#[cfg_attr(all(feature = "fb", not(target_os = "linux")), allow(dead_code))]
fn resolve_hostname() -> String {
    if let Ok(s) = std::env::var("HOSTNAME") {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(os) = nix::unistd::gethostname() {
            if let Ok(s) = os.into_string() {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    "workplacesim".to_string()
}

/// Resolves on SIGINT (Ctrl+C) or SIGTERM (systemd stop). On non-unix,
/// falls through to just Ctrl+C. Only compiled into the server-only branch;
/// desktop and fb branches run synchronously and handle signals locally.
#[cfg(not(any(feature = "desktop", feature = "fb")))]
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Bootstrap the axum server on a background tokio runtime. Used by both the
/// desktop (minifb main-thread constraint) and fb (render on main thread)
/// paths. The process exits on window close / SIGINT; cancellation of the
/// server is acceptable-abrupt for MVP.
#[cfg(any(feature = "desktop", all(feature = "fb", target_os = "linux")))]
fn spawn_server(
    addr: SocketAddr,
    app: AppState,
    hostname: String,
) -> std::thread::JoinHandle<anyhow::Result<()>> {
    std::thread::spawn(move || -> anyhow::Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async move {
            tracing::info!("workplacesim listening on http://{addr}");
            // Park the server until Ctrl+C; fb signal handler and minifb
            // window close also force-exit via process::exit from the
            // renderer branch.
            let shutdown = async {
                let _ = tokio::signal::ctrl_c().await;
            };
            workplacesim::server::run(addr, app, &hostname, shutdown).await
        })
    })
}

#[cfg(feature = "desktop")]
fn main() -> anyhow::Result<()> {
    bootstrap_logging();
    let addr = bind_addr();

    let (shared_config, config_path, config_source) = load_shared_config();

    let (state, rx) = workplacesim::state::new_state(shared_config.clone());
    // The render thread owns this receiver; SSE clients subscribe separately
    // via `State::subscribe_events()` when they connect.

    if let Some(n) = parse_demo_count() {
        seed_demo_agents(&state, n);
    }

    let app = build_app_state(
        state.clone(),
        shared_config.clone(),
        config_path,
        config_source,
    );
    let fb_info_handle = app.fb_info.clone();

    // minifb on macOS requires the main thread for the window (AppKit
    // constraint). Spawn the axum server on a background tokio runtime and
    // run the window loop on the main thread.
    let _server_thread = spawn_server(addr, app, resolve_hostname());

    workplacesim::render::desktop::run_desktop_with_fb_info(
        state,
        shared_config,
        rx,
        Some(fb_info_handle),
    )?;
    // Window closed → drop the server thread along with the process. MVP
    // accepts the abrupt shutdown; a proper cancellation channel is a future
    // polish.
    std::process::exit(0);
}

#[cfg(all(feature = "fb", target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    bootstrap_logging();
    let addr = bind_addr();

    let (shared_config, config_path, config_source) = load_shared_config();

    let (state, rx) = workplacesim::state::new_state(shared_config.clone());
    let app = build_app_state(
        state.clone(),
        shared_config.clone(),
        config_path,
        config_source,
    );
    let fb_info_handle = app.fb_info.clone();
    let _server_thread = spawn_server(addr, app, resolve_hostname());

    // Renderer runs on the main thread so signal delivery and VtGuard drop
    // order are predictable. `run_fb` installs its own SIGINT/SIGTERM handler
    // and polls a shared flag to exit the loop cleanly.
    workplacesim::render::fb::run_fb_with_fb_info(state, shared_config, rx, Some(fb_info_handle))?;
    std::process::exit(0);
}

// Building with --features fb on a non-Linux host produces a clean error at
// main() rather than cryptic ioctl/libc symbol failures deeper down.
#[cfg(all(feature = "fb", not(target_os = "linux")))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!(
        "feature `fb` requires target_os=\"linux\". For macOS dev use:\n  \
         cargo run --features desktop --no-default-features\n\
         Cross-compile for Pi 1:\n  \
         cross build --target arm-unknown-linux-gnueabihf --release --features fb --no-default-features"
    )
}

#[cfg(not(any(feature = "desktop", feature = "fb")))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap_logging();
    let addr = bind_addr();

    let (shared_config, config_path, config_source) = load_shared_config();

    let (state, _rx) = workplacesim::state::new_state(shared_config.clone());
    // Server-only branch: holding `_rx` keeps the broadcast channel primed so
    // `Sender::send` doesn't error when there are zero SSE clients connected.
    // Each SSE client spawns its own receiver via `State::subscribe_events()`.

    let app = build_app_state(state, shared_config, config_path, config_source);
    let hostname = resolve_hostname();

    tracing::info!("workplacesim listening on http://{addr}");
    workplacesim::server::run(addr, app, &hostname, shutdown_signal()).await
}
