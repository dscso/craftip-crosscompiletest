use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::sync::mpsc;
use crate::socket_packet::SocketPacket;

pub type Tx = mpsc::UnboundedSender<SocketPacket>;
pub type Rx = mpsc::UnboundedReceiver<SocketPacket>;

#[derive(Debug, Error)]
pub enum DistributorError {
    #[error("ClientNotFound")]
    ClientNotFound,
    #[error("Server Not found")]
    ServerNotFound,
    #[error("ClientAlreadyConnected")]
    ClientAlreadyConnected,
    #[error("ServerAlreadyConnected")]
    ServerAlreadyConnected,
    #[error("ClientNotConnected")]
    ClientNotConnected,
    #[error("ServerNotConnected")]
    ServerNotConnected,
    #[error("TooManyClients")]
    TooManyClients,
}

pub struct Distributor {
    pub clients: HashMap<SocketAddr, (Tx, String)>,
    pub servers: HashMap<String, Tx>,
    pub clients_server: HashMap<SocketAddr, String>,
    pub server_clients: HashMap<String, Vec<Option<SocketAddr>>>,
}

impl Distributor {
    pub fn new() -> Self {
        Distributor {
            clients: HashMap::new(),
            servers: HashMap::new(),
            clients_server: HashMap::new(),
            server_clients: HashMap::new(),
        }
    }
    /// adds the client to the distributor and returns the client id
    pub fn add_client(
        &mut self,
        addr: SocketAddr,
        hostname: &str,
        tx: Tx,
    ) -> Result<u32, DistributorError> {
        self.clients.insert(addr, (tx, hostname.to_string()));
        let server = self.servers.get_mut(hostname);
        if server.is_none() {
            return Err(DistributorError::ServerNotFound);
        }
        let mut id = 0;
        for client in self.server_clients.get_mut(hostname).unwrap() {
            if client.is_none() {
                *client = Some(addr);
                return Ok(id);
            }
            id += 1;
        }
        Err(DistributorError::TooManyClients)
    }
    /// adds the server to the distributor
    pub fn add_server(&mut self, hostname: &str, tx: Tx) -> Result<(), DistributorError> {
        self.servers.insert(hostname.to_string(), tx);
        let mut sockets: Vec<Option<SocketAddr>> = (0..100).map(|_| None).collect();
        self.server_clients.insert(hostname.to_string(), sockets);
        Ok(())
    }

    pub fn remove_client(&mut self, addr: SocketAddr) {
        let (tx, hostname) = self.clients.remove(&addr).unwrap();
        let server = self.servers.get_mut(&hostname).unwrap();
        let mut id = 0;
        for client in self.server_clients.get_mut(&hostname).unwrap() {
            if client.is_some() && client.unwrap() == addr {
                *client = None;
                return;
            }
            id += 1;
        }
    }
    pub fn remove_server(&mut self, hostname: &str) {
        self.servers.remove(hostname);
        for client in self.server_clients.get_mut(hostname).unwrap() {
            if client.is_some() {
                let client = self.clients.remove(client.as_ref().unwrap());
                if client.is_some() {
                    let (tx, _) = client.unwrap();
                    // todo disconnect
                }
            }
        }
        self.server_clients.remove(hostname);
    }

    async fn send_to_server(&mut self, server: String, packet: SocketPacket) {
        for peer in self.servers.iter_mut() {
            tracing::info!("MC -> Server");
            if *peer.0 == server {
                let _ = peer.1.send(packet.clone());
            }
        }
    }

    async fn send_to_client(&mut self, client: String, buf: SocketPacket) {
        for peer in self.clients.iter_mut() {
            //if *peer.0 == client {
            //let _ = peer.1.send(buf.clone());
            //}
        }
    }
}
