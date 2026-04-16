use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4317);
    let host: IpAddr = std::env::var("HOST")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let addr = SocketAddr::new(host, port);

    let (state, _rx) = workplacesim::state::new_state();
    // TODO(step 7): wire broadcast receiver to SSE clients; until then, hold
    // `_rx` so broadcast::Sender::send never fails for "no receivers".

    tracing::info!("workplacesim listening on http://{addr}");
    workplacesim::server::run(addr, state, shutdown_signal()).await
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
