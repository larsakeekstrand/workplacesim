//! In-process mDNS responder.
//!
//! Replaces the external `avahi-daemon` + static service XML the Pi deploy
//! used to ship. Registers `_workplacesim._tcp.local.` and `_http._tcp.local.`
//! on the bound port so `workplacesim.local` resolves without any sidecar
//! service.
//!
//! Fire-and-forget: any registration failure logs a warning and returns a
//! no-op guard. mDNS is nice-to-have, never load-bearing.

use std::net::SocketAddr;

/// Opaque handle kept alive for the lifetime of the server. Dropping it
/// unregisters the services and shuts down the mDNS daemon. On non-Linux
/// targets this is a zero-sized no-op.
pub struct MdnsGuard {
    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    inner: Option<self::linux::Registration>,
}

impl MdnsGuard {
    /// A guard that owns nothing. Used when registration fails or on
    /// platforms where the in-process responder is disabled.
    pub fn noop() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            inner: None,
        }
    }
}

/// Register the two service types advertised by the old avahi XML.
///
/// `addr` is the actually-bound socket address; pass the result of
/// `TcpListener::local_addr()` so ephemeral-port tests advertise the real
/// port. `hostname` is the bare host label (no `.local.` suffix — the
/// responder appends it).
pub fn register(addr: SocketAddr, hostname: &str) -> MdnsGuard {
    #[cfg(target_os = "linux")]
    {
        match self::linux::Registration::new(addr, hostname) {
            Ok(reg) => MdnsGuard { inner: Some(reg) },
            Err(e) => {
                tracing::warn!("mdns registration failed: {e}; continuing without mDNS");
                MdnsGuard::noop()
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (addr, hostname);
        MdnsGuard::noop()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::net::SocketAddr;

    use mdns_sd::{ServiceDaemon, ServiceInfo};

    const WORKPLACESIM_SERVICE: &str = "_workplacesim._tcp.local.";
    const HTTP_SERVICE: &str = "_http._tcp.local.";

    pub(super) struct Registration {
        daemon: ServiceDaemon,
        fullnames: Vec<String>,
    }

    impl Registration {
        pub(super) fn new(addr: SocketAddr, hostname: &str) -> anyhow::Result<Self> {
            let daemon = ServiceDaemon::new()
                .map_err(|e| anyhow::anyhow!("ServiceDaemon::new failed: {e}"))?;

            let instance = sanitize_instance(hostname);
            let host = format!("{instance}.local.");
            let port = addr.port();

            let mut fullnames = Vec::with_capacity(2);

            // Mirrors the old workplacesim.avahi-service XML verbatim.
            let ws = build_service(
                WORKPLACESIM_SERVICE,
                &instance,
                &host,
                addr,
                port,
                &[("path", "/events"), ("version", "1")],
            )?;
            fullnames.push(ws.get_fullname().to_string());
            daemon
                .register(ws)
                .map_err(|e| anyhow::anyhow!("register {WORKPLACESIM_SERVICE}: {e}"))?;

            let http = build_service(
                HTTP_SERVICE,
                &instance,
                &host,
                addr,
                port,
                &[("path", "/")],
            )?;
            fullnames.push(http.get_fullname().to_string());
            daemon
                .register(http)
                .map_err(|e| anyhow::anyhow!("register {HTTP_SERVICE}: {e}"))?;

            tracing::info!(
                "mdns registered {WORKPLACESIM_SERVICE} + {HTTP_SERVICE} as {instance} on port {port}"
            );
            Ok(Self { daemon, fullnames })
        }
    }

    fn build_service(
        service_type: &str,
        instance: &str,
        host: &str,
        addr: SocketAddr,
        port: u16,
        txt: &[(&str, &str)],
    ) -> anyhow::Result<ServiceInfo> {
        // Empty `ip` + `enable_addr_auto()` lets the daemon pick up whatever
        // non-loopback NIC addresses exist at register time and refresh as
        // they change. For bind addresses that aren't wildcard we still add
        // them explicitly so loopback-bound test servers are discoverable on
        // localhost without racing the auto-detect sweep.
        let info = if addr.ip().is_unspecified() {
            ServiceInfo::new(service_type, instance, host, "", port, txt)
                .map_err(|e| anyhow::anyhow!("ServiceInfo::new: {e}"))?
                .enable_addr_auto()
        } else {
            ServiceInfo::new(
                service_type,
                instance,
                host,
                addr.ip().to_string().as_str(),
                port,
                txt,
            )
            .map_err(|e| anyhow::anyhow!("ServiceInfo::new: {e}"))?
        };
        Ok(info)
    }

    /// mDNS instance labels are DNS labels: printable, no dots/spaces, <=63
    /// bytes. Most sane hostnames already satisfy this; guard against the
    /// odd FQDN or weird char just in case.
    fn sanitize_instance(hostname: &str) -> String {
        let mut s: String = hostname
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if s.is_empty() {
            s.push_str("workplacesim");
        }
        if s.len() > 63 {
            s.truncate(63);
        }
        s
    }

    impl Drop for Registration {
        fn drop(&mut self) {
            for fullname in &self.fullnames {
                match self.daemon.unregister(fullname) {
                    Ok(_) => {}
                    Err(e) => tracing::warn!("mdns unregister {fullname}: {e}"),
                }
            }
            // Best-effort shutdown; we can't block here because Drop has to
            // return promptly and ServiceDaemon::shutdown returns a receiver.
            let _ = self.daemon.shutdown();
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::{Duration, Instant};

    use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent};

    use super::*;

    fn unique_hostname(tag: &str) -> String {
        format!("workplacesim-test-{tag}-{}", std::process::id())
    }

    fn wait_for_resolution(
        rx: &Receiver<ServiceEvent>,
        expected: &str,
        budget: Duration,
    ) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            match rx.recv_timeout(left) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    if info.get_fullname().contains(expected) {
                        return true;
                    }
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    }

    fn wait_for_removal(
        rx: &Receiver<ServiceEvent>,
        expected: &str,
        budget: Duration,
    ) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            match rx.recv_timeout(left) {
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    if fullname.contains(expected) {
                        return true;
                    }
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    }

    #[test]
    fn register_is_discoverable_and_unregisters_on_drop() {
        let hostname = unique_hostname("discover");
        // Bind a real TCP listener on an ephemeral port so the registered
        // port mirrors what axum would see in production.
        let listener = std::net::TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
        ))
        .expect("bind ephemeral");
        let addr = listener.local_addr().expect("local_addr");

        let browser = ServiceDaemon::new().expect("browser daemon");
        let rx = browser
            .browse("_workplacesim._tcp.local.")
            .expect("browse start");

        let guard = register(addr, &hostname);

        assert!(
            wait_for_resolution(&rx, &hostname, Duration::from_secs(3)),
            "registered service was not discovered within 3s"
        );

        drop(guard);

        assert!(
            wait_for_removal(&rx, &hostname, Duration::from_secs(3)),
            "unregistered service did not produce a removal event within 3s"
        );

        let _ = browser.shutdown();
    }
}
