use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tracing_subscriber::EnvFilter;

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
    for i in 0..n {
        let (user, ty, mode, desc) = profiles[i % profiles.len()];
        let id = format!("demo-{i}");
        let sid = format!("demo-session-{i}");
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

/// Resolves on SIGINT (Ctrl+C) or SIGTERM (systemd stop). On non-unix,
/// falls through to just Ctrl+C.
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

#[cfg(feature = "desktop")]
fn main() -> anyhow::Result<()> {
    bootstrap_logging();
    let addr = bind_addr();

    let (state, rx) = workplacesim::state::new_state();
    // The render thread now owns this receiver. SSE (step 7) will subscribe
    // separately via `State::subscribe()`.

    if let Some(n) = parse_demo_count() {
        seed_demo_agents(&state, n);
    }

    // minifb on macOS requires the main thread for the window (AppKit
    // constraint). Spawn the axum server on a background tokio runtime and
    // run the window loop on the main thread.
    let server_state = state.clone();
    let _server_thread = std::thread::spawn(move || -> anyhow::Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async move {
            tracing::info!("workplacesim listening on http://{addr}");
            workplacesim::server::run(addr, server_state, shutdown_signal()).await
        })
    });

    workplacesim::render::desktop::run_desktop(state, rx)?;
    // Window closed → drop the server thread along with the process. Step 4a
    // accepts the abrupt shutdown; step 7 will wire a proper cancellation
    // channel.
    std::process::exit(0);
}

#[cfg(not(feature = "desktop"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap_logging();
    let addr = bind_addr();

    let (state, _rx) = workplacesim::state::new_state();
    // TODO(step 7): wire broadcast receiver to SSE clients; until then, hold
    // `_rx` so broadcast::Sender::send never fails for "no receivers".

    tracing::info!("workplacesim listening on http://{addr}");
    workplacesim::server::run(addr, state, shutdown_signal()).await
}
