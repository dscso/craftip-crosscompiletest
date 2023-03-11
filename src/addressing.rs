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
    ) -> Result<u16, DistributorError> {
        self.clients.insert(addr, (tx, hostname.to_string()));
        let mut id = 1;
        for client in self.server_clients.get_mut(hostname).ok_or(DistributorError::ServerNotFound)? {
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
        if self.servers.contains_key(hostname) {
            return Err(DistributorError::ServerAlreadyConnected);
        }
        self.servers.insert(hostname.to_string(), tx);
        let sockets: Vec<Option<SocketAddr>> = (0..100).map(|_| None).collect();
        self.server_clients.insert(hostname.to_string(), sockets);
        Ok(())
    }

    pub fn remove_client(&mut self, addr: &SocketAddr) -> Result<(), DistributorError> {
        let (_tx, hostname) = self.clients.remove(&addr).ok_or(DistributorError::ClientNotFound)?;
        let mut id = 0;
        for client in self.server_clients.get_mut(&hostname).ok_or(DistributorError::ServerNotFound)? {
            if *client == Some(*addr) {
                *client = None;
                return Ok(());
            }
            id += 1;
        }
        Err(DistributorError::ClientNotFound)
    }
    pub fn remove_server(&mut self, hostname: &str) -> Result<(), DistributorError> {
        self.servers.remove(hostname);
        for client in self.server_clients.get_mut(hostname).ok_or(DistributorError::ServerNotFound)? {
            if client.is_some() {
                let client = self.clients.remove(client.as_ref().ok_or(DistributorError::ClientNotFound)?);
                if let Some(client) = client {
                    let (tx, _) = client;
                    // todo disconnect
                    tx.send(SocketPacket::UnknownPacket).map_err(|_| (DistributorError::ClientNotFound))?;
                }
            }
        }
        self.server_clients.remove(hostname);
        Ok(())
    }

    pub fn send_to_server(&mut self, server: &str, packet: &SocketPacket) {
        for peer in self.servers.iter_mut() {
            tracing::info!("MC -> Server");
            if *peer.0 == server {
                let _ = peer.1.send(packet.clone());
            }
        }
    }

    pub fn send_to_client(&mut self, hostname: &str, client_id: u16, packet: &SocketPacket) {
        match self.server_clients.get(hostname) {
            Some(clients) => {
                if let Some(client) = clients.get(client_id as usize) {
                    if let Some(client) = client {
                        let client = self.clients.get_mut(client).unwrap();
                        tracing::info!("MC -> Client");
                        let _ = client.0.send(packet.clone());
                    }
                }
            }
            None => { println!("Server not found") }
        }
    }
}
