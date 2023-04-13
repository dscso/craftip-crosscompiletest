use crate::socket_packet::{ChannelMessage, SocketPacket};
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::sync::mpsc;

pub type Tx = mpsc::UnboundedSender<ChannelMessage<SocketPacket>>;
pub type Rx = mpsc::UnboundedReceiver<ChannelMessage<SocketPacket>>;

#[derive(Debug, Error, PartialEq, Eq)]
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
    #[error("UnknownError")]
    UnknownError,
}

type ServerHostname = String;

#[derive(Debug)]
pub struct Distributor {
    pub clients: HashMap<SocketAddr, (Tx, ServerHostname)>,
    pub servers: HashMap<ServerHostname, Tx>,
    pub server_clients: HashMap<ServerHostname, Vec<Option<SocketAddr>>>,
}

impl Distributor {
    pub fn new() -> Self {
        Distributor {
            clients: HashMap::new(),
            servers: HashMap::new(),
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
        let server_clients = self
            .server_clients
            .get_mut(hostname)
            .ok_or(DistributorError::ServerNotFound)?;

        for (id, client) in server_clients.iter_mut().enumerate() {
            if client.is_none() {
                *client = Some(addr);
                // if everything worked, add client and return OK
                self.clients.insert(addr, (tx, hostname.to_string()));
                return Ok(id as u16);
            }
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
        let (_, hostname) = self
            .clients
            .remove(addr)
            .ok_or(DistributorError::ClientNotFound)?;

        if let Some(clients) = self.server_clients.get_mut(&hostname) {
            for client in clients {
                if let Some(c) = client {
                    if *c == *addr {
                        *client = None;
                        return Ok(());
                    }
                }
            }
        }
        Err(DistributorError::ClientNotFound)
    }
    pub fn remove_server(&mut self, hostname: &str) -> Result<(), DistributorError> {
        self.servers.remove(hostname);
        for client in self
            .server_clients
            .get_mut(hostname)
            .ok_or(DistributorError::ServerNotFound)?
        {
            if client.is_some() {
                let client = self
                    .clients
                    .remove(client.as_ref().ok_or(DistributorError::ClientNotFound)?);
                if let Some(client) = client {
                    let (tx, _) = client;
                    tx.send(ChannelMessage::Close)
                        .map_err(|_| (DistributorError::ClientNotFound))?;
                }
            }
        }
        self.server_clients.remove(hostname);
        Ok(())
    }

    pub fn send_to_server(
        &mut self,
        server: &str,
        packet: SocketPacket,
    ) -> Result<(), DistributorError> {
        for peer in self.servers.iter_mut() {
            tracing::debug!("MC -> Server");
            if *peer.0 == server {
                let _ = peer.1.send(ChannelMessage::Packet(packet));
                return Ok(());
            }
        }
        Err(DistributorError::ServerNotFound)
    }

    pub fn send_to_client(
        &mut self,
        hostname: &str,
        client_id: u16,
        packet: &SocketPacket,
    ) -> Result<(), DistributorError> {
        let client = self.get_client(hostname, client_id)?;
        tracing::debug!("MC -> Client");
        if let Err(e) = client.send(ChannelMessage::Packet(packet.clone())) {
            tracing::error!("could not send: {}", e);
            return Err(DistributorError::UnknownError);
        }
        Ok(())
    }
    pub fn get_client(
        &mut self,
        hostname: &str,
        client_id: u16,
    ) -> Result<&mut Tx, DistributorError> {
        match self.server_clients.get(hostname) {
            Some(clients) => {
                if let Some(Some(client)) = clients.get(client_id as usize) {
                    let client = self
                        .clients
                        .get_mut(client)
                        .expect("Error in distributor send_to_client");
                    return Ok(&mut client.0);
                }
                Err(DistributorError::ClientNotFound)
            }
            None => Err(DistributorError::ServerNotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_add_client() {
        let mut distributor = Distributor::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234);
        let tx = mpsc::unbounded_channel().0;

        // add server
        distributor.add_server("localhost", tx.clone()).unwrap();

        // add client
        let client_id = distributor
            .add_client(addr, "localhost", tx.clone())
            .unwrap();
        assert_eq!(client_id, 0);

        // add another client
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1235);
        let client_id = distributor
            .add_client(addr2, "localhost", tx.clone())
            .unwrap();
        assert_eq!(client_id, 1);

        // too many clients
        for i in 2..=99 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234 + i);
            let result = distributor.add_client(addr, "localhost", tx.clone());
            assert_eq!(result, Ok(i));
        }
    }

    #[test]
    fn test_add_server() {
        let mut distributor = Distributor::new();
        let tx = mpsc::unbounded_channel().0;
        // add server
        distributor.add_server("localhost", tx.clone()).unwrap();
        assert!(distributor.servers.contains_key("localhost"));
        assert!(distributor.server_clients.contains_key("localhost"));
        assert_eq!(
            distributor.server_clients.get("localhost").unwrap().len(),
            100
        );

        // add duplicate server
        let result = distributor.add_server("localhost", tx);
        assert_eq!(result, Err(DistributorError::ServerAlreadyConnected));
    }

    #[test]
    fn test_remove_client() {
        let mut distributor = Distributor::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234);
        let tx = mpsc::unbounded_channel().0;

        // add server
        distributor.add_server("localhost", tx.clone()).unwrap();

        // add client
        let result = distributor
            .add_client(addr, "localhost", tx.clone())
            .unwrap();
        assert_eq!(result, 0);

        // too many clients
        for i in 1..=99 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234 + i);
            let result = distributor.add_client(addr, "localhost", tx.clone());
            assert_eq!(result, Ok(i));
        }

        for i in 0..=99 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234 + i);
            let result = distributor.get_client("localhost", i);
            assert!(result.is_ok());
        }

        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9999);
        let tx = mpsc::unbounded_channel().0;
        let result = distributor.add_client(addr1, "localhost", tx);
        assert_eq!(result, Err(DistributorError::TooManyClients));

        // remove client
        distributor.remove_client(&addr).unwrap();

        let result = distributor.server_clients.get("localhost").unwrap()[0];
        assert_eq!(result, None);

        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1000);
        let tx = mpsc::unbounded_channel().0;
        let result = distributor.add_client(addr2, "localhost", tx);
        assert_eq!(result, Ok(0));

        assert!(!distributor.clients.is_empty());

        // remove non-existent client
        let addr_non_existent = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4321);
        let result = distributor.remove_client(&addr_non_existent);
        assert_eq!(result, Err(DistributorError::ClientNotFound));
    }

    #[test]
    fn test_remove_server() {
        let mut distributor = Distributor::new();
        let (tx, rx) = mpsc::unbounded_channel();
        let (tx_cli, mut rx_cli) = mpsc::unbounded_channel();

        // add server
        distributor.add_server("localhost", tx.clone()).unwrap();
        distributor.add_server("localhost2", tx).unwrap();
        // add clients
        for i in 0..=99 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234 + i);
            let result = distributor.add_client(addr, "localhost", tx_cli.clone());
            assert_eq!(result, Ok(i));
        }
        for i in 0..=99 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 2000 + i);
            let result = distributor.add_client(addr, "localhost2", tx_cli.clone());
            assert_eq!(result, Ok(i));
        }
        // remove server
        distributor.remove_server("localhost").unwrap();
        let mut count = 0;
        while let Ok(result) = rx_cli.try_recv() {
            assert_eq!(result, ChannelMessage::Close);
            count += 1;
        }
        assert_eq!(count, 100);
        assert!(!distributor.servers.contains_key("localhost"));
        assert!(distributor.server_clients.contains_key("localhost2"));

        for i in 0..=99 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 2000 + i);
            distributor.get_client("localhost2", i).unwrap();
            let result = distributor.remove_client(&addr);
            assert_eq!(result, Ok(()));
        }

        let mut count = 0;
        while let Ok(result) = rx_cli.try_recv() {
            assert_eq!(result, ChannelMessage::Close);
            count += 1;
        }
        distributor.remove_server("localhost2").unwrap();
        assert_eq!(count, 0);
        assert!(distributor.servers.is_empty());
        assert!(distributor.server_clients.is_empty());

        // remove non-existent server
        let result = distributor.remove_server("localhost");
        assert_eq!(result, Err(DistributorError::ServerNotFound));
    }
}
