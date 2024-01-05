use std::collections::HashMap;
use std::time::Duration;
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use futures::SinkExt;
use thiserror::Error;
use tokio::io;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use shared::packet_codec::{PacketCodec, PacketCodecError};
use shared::proxy::{ProxyAuthenticator, ProxyHelloPacket};
use shared::socket_packet::SocketPacket;

use crate::connection_handler::ClientConnection;
use crate::gui::gui_channel::Server;
use crate::ServerAuthentication;

#[derive(Debug)]
pub enum Stats {
    Connected,
    ClientsConnected(u16),
    Ping(u16),
    Disconnected,
}

#[derive(Debug)]
pub enum Control {
    Disconnect,
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Io Error: {0}")]
    Io(#[from] io::Error),
    #[error("protocol error: {0}")]
    ProtocolError(#[from] PacketCodecError),
    #[error("Proxy closed the connection")]
    ProxyClosedConnection,
    #[error("User closed the connection")]
    UserClosedConnection,
    #[error("Proxy error: {0}")]
    ProxyError(String),
    #[error("Minecraft server error. Is the server running?")]
    MinecraftServerNotFound,
    #[error("Unexpected packet: {0}")]
    UnexpectedPacket(String),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
pub type Tx = mpsc::UnboundedSender<Option<SocketPacket>>;
pub type Rx = mpsc::UnboundedReceiver<Option<SocketPacket>>;

pub type ClientTx = mpsc::UnboundedSender<Option<Vec<u8>>>;
pub type ClientRx = mpsc::UnboundedReceiver<Option<Vec<u8>>>;

pub type ControlTx = mpsc::UnboundedSender<Control>;
pub type ControlRx = mpsc::UnboundedReceiver<Control>;

pub type StatsTx = mpsc::UnboundedSender<Stats>;
pub type StatsRx = mpsc::UnboundedReceiver<Stats>;

pub struct Client {
    state: Shared,
    stats_tx: StatsTx,
    proxy: Option<Framed<TcpStream, PacketCodec>>,
    control_rx: ControlRx,
    server: Server,
}

pub struct Shared {
    connections: HashMap<u16, mpsc::UnboundedSender<Option<Vec<u8>>>>,
    stats_tx: Option<StatsTx>,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub fn new() -> Self {
        Shared {
            connections: HashMap::new(),
            stats_tx: None,
        }
    }
    pub fn set_stats_tx(&mut self, tx: StatsTx) {
        self.stats_tx = Some(tx);
    }
    pub fn add_connection(&mut self, id: u16, tx: mpsc::UnboundedSender<Option<Vec<u8>>>) {
        self.connections.insert(id, tx);
        if let Some(tx) = &self.stats_tx {
            tx.send(Stats::ClientsConnected(self.connections.len() as u16))
                .unwrap();
        }
    }
    pub fn remove_connection(&mut self, id: u16) {
        self.connections.remove(&id);
        if let Some(tx) = &self.stats_tx {
            tx.send(Stats::ClientsConnected(self.connections.len() as u16))
                .unwrap();
        }
    }
    pub fn send_to(&mut self, id: u16, msg: Option<Vec<u8>>) -> Result<()> {
        let channel = self
            .connections
            .get_mut(&id)
            .context(format!("could not find client id {}, {:?}", id, msg))?;
        channel.send(msg).unwrap_or_else(|e| {
            self.connections.remove(&id);
        });
        Ok(())
    }
}

impl Client {
    pub async fn new(server: Server, stats_tx: StatsTx, mut control_rx: ControlRx) -> Self {
        let mut state = Shared::new();
        state.set_stats_tx(stats_tx.clone());
        Client {
            server,
            stats_tx,
            state,
            control_rx,
            proxy: None,
        }
    }
}

impl Client {
    pub async fn connect(&mut self) -> Result<(), ClientError> {
        // test connection to minecraft server
        TcpStream::connect(&self.server.local)
            .await
            .map_err(|_| ClientError::MinecraftServerNotFound)?;
        // connect to proxy
        let proxy_stream = TcpStream::connect(format!("{}:25565", &self.server.server)).await?;
        let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 4));

        let hello = SocketPacket::from(ProxyHelloPacket {
            version: 123,
            hostname: self.server.server.clone(),
            auth: match &mut self.server.auth {
                ServerAuthentication::Key(private_key) => {
                    ProxyAuthenticator::PublicKey(private_key.get_public_key())
                }
            },
        });

        proxy.send(hello).await?;

        let packet = proxy.next().await.unwrap().unwrap();

        if let SocketPacket::ProxyAuthRequest(challenge) = packet {
            match &mut self.server.auth {
                ServerAuthentication::Key(private_key) => {
                    let mut signature = private_key.sign(&challenge);
                    proxy
                        .send(SocketPacket::ProxyAuthResponse(signature))
                        .await?;
                }
            }
        } else {
            return Err(ClientError::UnexpectedPacket(format!("{:?}", packet)));
        }

        tokio::select! {
            res = proxy.next() => match res {
                Some(Ok(SocketPacket::ProxyHelloResponse(hello_response))) => {},
                Some(Ok(SocketPacket::ProxyError(e))) => return Err(ClientError::ProxyError(e)),
                None => return Err(ClientError::ProxyClosedConnection),
                Some(Err(e)) => return Err(ClientError::ProtocolError(e)),
                e => return Err(ClientError::UnexpectedPacket(format!("{:?}", e))),
            },
            res = self.control_rx.recv() => match res {
                Some(Control::Disconnect) | None => {
                    return Err(ClientError::UserClosedConnection)
                }
            }
        }
        tracing::info!("Connected to proxy server!");
        self.stats_tx
            .send(Stats::Connected)
            .map_err(|e| ClientError::Other(e.into()))?;
        self.proxy = Some(proxy);
        Ok(())
    }
    pub async fn handle(&mut self) -> Result<()> {
        let (to_proxy_tx, mut to_proxy_rx) = mpsc::unbounded_channel();
        let (client_handler_death_tx, mut client_handler_death_rx) =
            mpsc::unbounded_channel::<String>();
        let mut proxy = self.proxy.as_mut().unwrap();
        loop {
            tokio::select! {
                // process control messages e.g. form gui
                result = self.control_rx.recv() => {
                    match result {
                        Some(Control::Disconnect) | None => {
                            return Ok(());
                        }
                    }
                }
                // if any client handler dies, return error
                result = client_handler_death_rx.recv() => {
                    match result {
                        Some(e) => bail!(e),
                        None => bail!("client handler died")
                    }
                }
                // send packets to proxy
                Some(pkg) = to_proxy_rx.recv() => {
                    //tracing::info!("Sending packet to client: {:?}", pkg);
                    match pkg {
                        Some(pkg) => {
                            proxy.send(pkg).await?;
                        }
                        None => bail!("all clients dropped")
                    }
                }
                // receive proxy packets
                result = proxy.next() => {
                    match result {
                        Some(Ok(msg)) => {
                            match msg {
                                SocketPacket::ProxyJoin(packet) => {
                                    let (mut client_connection, client_tx) = ClientConnection::new(to_proxy_tx.clone(), self.server.local.clone(), packet.client_id).await;
                                    self.state.add_connection(packet.client_id, client_tx);
                                    let client_handler_death_tx = client_handler_death_tx.clone();
                                    tokio::spawn(async move {
                                        client_connection.handle_client().await.unwrap_or_else(|e| {
                                            tracing::error!("An Error occurred in the handle_client function: {}", e);
                                            // sometimes handle_client closes after gui, errors can occur
                                            let _res = client_handler_death_tx.send(e.to_string());
                                        });

                                        client_connection.close().await;
                                    });
                                }
                                SocketPacket::ProxyData(packet) => {
                                    self.state.send_to(packet.client_id, Some(packet.data.to_vec()))?;
                                }
                                SocketPacket::ProxyDisconnect(packet) => {
                                    // this can fail if the client is already disconnected
                                    let _ = self.state.send_to(packet.client_id, None);
                                    self.state.remove_connection(packet.client_id);
                                }
                                SocketPacket::ProxyPong(ping) => {
                                    let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u16;
                                    let ping = time.saturating_sub(ping);
                                    self.stats_tx.send(Stats::Ping(ping))?;
                                }
                                _ => unimplemented!("Message not implemented!")
                            }
                        }
                        // An error occurred.
                        Some(Err(e)) => bail!("an error occurred while processing messages error = {:?}", e),
                        // The stream has been exhausted.
                        None => bail!("Proxy has closed the connection")
                    }
                },
                // ensure constant traffic so tcp connection does not close
                _ = sleep(Duration::from_secs(1)) => {
                    let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u16;
                    proxy.send(SocketPacket::ProxyPing(time)).await?;
                    continue;
                }
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        tracing::info!("Client dropped");
    }
}
