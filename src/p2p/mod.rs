pub mod discovery;
pub mod server;

use anyhow::Result;
use mdns_sd::ServiceDaemon;
use std::sync::Arc;
use std::time::Duration;

use crate::db::Database;
use crate::resolver::http::HttpResolver;
use crate::resolver::Resolver;
use server::PeerServer;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Node {
    pub node_id: String,
    pub server_port: u16,
    mdns: ServiceDaemon,
    service_fullname: String,
    _server: PeerServer,
}

impl Node {
    pub fn start(db: Arc<Database>, node_id: String) -> Result<Self> {
        let server = PeerServer::start(Arc::clone(&db))?;
        let port = server.port;

        let mdns =
            ServiceDaemon::new().map_err(|e| anyhow::anyhow!("start mDNS daemon: {e}"))?;
        let fullname = discovery::announce(&mdns, &node_id, port)?;

        tracing::info!(port, %node_id, "peer node started");

        Ok(Node {
            node_id,
            server_port: port,
            mdns,
            service_fullname: fullname,
            _server: server,
        })
    }

    pub fn discover_peers(&self) -> Result<Vec<HttpResolver>> {
        let peers = discovery::browse_peers(&self.mdns, &self.node_id, DISCOVERY_TIMEOUT)?;
        let mut resolvers = Vec::new();
        for base_url in peers {
            match HttpResolver::connect_peer(base_url.clone()) {
                Ok(r) => {
                    tracing::info!(peer = %base_url, name = r.name(), "discovered peer");
                    resolvers.push(r);
                }
                Err(e) => {
                    tracing::warn!(peer = %base_url, error = %e, "could not connect to peer");
                }
            }
        }
        Ok(resolvers)
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.mdns.unregister(&self.service_fullname);
    }
}
