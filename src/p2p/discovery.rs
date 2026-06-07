use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

const SERVICE_TYPE: &str = "_hawkwing._tcp.local.";

/// Best-effort: find the local IPv4 address used for outbound traffic.
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

/// Announce this node via mDNS. Returns the fully-qualified service name
/// (needed for unregistration).
pub fn announce(mdns: &ServiceDaemon, node_id: &str, port: u16) -> Result<String> {
    let ip = local_ipv4().unwrap_or(Ipv4Addr::LOCALHOST);
    let host_name = format!("hawkwing-{}.local.", &node_id[..8]);
    let instance_name = format!("hawkwing-{node_id}");

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &host_name,
        IpAddr::V4(ip),
        port,
        None,
    )
    .map_err(|e| anyhow::anyhow!("build ServiceInfo: {e}"))?;

    let fullname = info.get_fullname().to_string();
    mdns.register(info)
        .map_err(|e| anyhow::anyhow!("register mDNS: {e}"))?;

    tracing::info!(ip = %ip, port, "mDNS service announced");
    Ok(fullname)
}

/// Browse the local network for Hawkwing peers for up to `timeout`.
/// Returns `(base_url)` strings, excluding our own node.
pub fn browse_peers(
    mdns: &ServiceDaemon,
    own_node_id: &str,
    timeout: Duration,
) -> Result<Vec<String>> {
    let receiver = mdns
        .browse(SERVICE_TYPE)
        .map_err(|e| anyhow::anyhow!("browse mDNS: {e}"))?;

    let mut peers: Vec<String> = Vec::new();
    let deadline = Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if info.get_fullname().contains(own_node_id) {
                    continue; // skip ourselves
                }
                let port = info.get_port();
                for addr in info.get_addresses() {
                    peers.push(format!("http://{}:{}", addr, port));
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    Ok(peers)
}
