use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use futures::SinkExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;
use std::time::SystemTime;

use anyhow::{Context, Result};

use shared::packet_codec::PacketCodec;
use shared::proxy::ProxyHelloPacket;
use shared::socket_packet::{ChannelMessage, SocketPacket};

use crate::connection_handler::ClientConnection;
#[derive(Debug)]
pub enum Stats {
    Connected,
    ClientsConnected(u16),
    Disconnected,
}

#[derive(Debug)]
pub enum Control {
    Disconnect,
}

pub type Tx = mpsc::UnboundedSender<ChannelMessage<SocketPacket>>;
pub type Rx = mpsc::UnboundedReceiver<ChannelMessage<SocketPacket>>;

pub type ControlTx = mpsc::UnboundedSender<Control>;
pub type ControlRx = mpsc::UnboundedReceiver<Control>;

pub type StatsTx = mpsc::UnboundedSender<Stats>;
pub type StatsRx = mpsc::UnboundedReceiver<Stats>;

#[derive(Clone)]
pub struct Client {
    proxy_server: String,
    mc_server: String,
    state: Arc<Mutex<Shared>>,
    stats_tx: StatsTx,
}

pub struct Shared {
    connections: HashMap<u16, mpsc::UnboundedSender<ChannelMessage<Vec<u8>>>>,
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
    pub fn add_connection(&mut self, id: u16, tx: mpsc::UnboundedSender<ChannelMessage<Vec<u8>>>) {
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
    pub fn get_connection(
        &mut self,
        id: u16,
    ) -> Option<&mut mpsc::UnboundedSender<ChannelMessage<Vec<u8>>>> {
        self.connections.get_mut(&id)
    }
}

impl Client {
    pub async fn new(proxy_server: String, mc_server: String, stats_tx: StatsTx) -> Self {
        let state = Arc::new(Mutex::new(Shared::new()));
        state.lock().await.set_stats_tx(stats_tx.clone());
        Client {
            proxy_server,
            mc_server,
            stats_tx,
            state,
        }
    }
}

impl Client {
    pub async fn connect(&mut self, mut control_rx: ControlRx) -> Result<(), Box<dyn Error>> {
        // todo good formatting
        let proxy_stream = TcpStream::connect(format!("{}:25565", &self.proxy_server)).await?;
        let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 4));

        let hello = SocketPacket::from(ProxyHelloPacket {
            version: 123,
            hostname: self.proxy_server.clone(),
        });
        proxy.send(hello).await?;
        self.stats_tx.send(Stats::Connected)?;
        tracing::info!("Connected to proxy server!");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (client_handler_death_tx, mut client_handler_death_rx) = mpsc::unbounded_channel::<String>();
        loop {
            tokio::select! {
                result = control_rx.recv() => {
                    match result {
                        Some(Control::Disconnect) | None => {
                            return Ok(());
                        }
                    }
                }
                result = client_handler_death_rx.recv() => {
                    match result {
                        Some(e) => {
                            return Err(e.into());
                        }
                        None => {
                            return Err("client handler died".into());
                        }
                    }
                }
                Some(pkg) = rx.recv() => {
                    //tracing::info!("Sending packet to client: {:?}", pkg);
                    match pkg {
                        ChannelMessage::Packet(pkg) => {
                            proxy.send(pkg).await?;
                        }
                        ChannelMessage::Close => {
                            return Err("all clients dropped".into());
                        }
                    }
                }
                result = proxy.next() => {
                    match result {
                        Some(Ok(msg)) => {
                            match msg {
                                SocketPacket::ProxyJoin(packet) => {
                                    let mut client_connection = ClientConnection::new(self.state.clone(), tx.clone(), self.mc_server.clone(), packet.client_id).await;
                                    let client_handler_death_tx = client_handler_death_tx.clone();
                                    tokio::spawn(async move {
                                        client_connection.handle_client().await.unwrap_or_else(|e| {
                                            tracing::error!("An Error occurred in the handle_client function: {}", e);
                                            // sometimes handle_client closes after gui, errors can occur
                                            let _res = client_handler_death_tx.send(e.to_string());
                                        })

                                        client_connection.close().await;
                                    });
                                }
                                SocketPacket::ProxyData(packet) => {
                                    match self.state.lock().await.get_connection(packet.client_id) {
                                        Some(tx) => {
                                            tx.send(ChannelMessage::Packet(packet.data.to_vec()))?;
                                        }
                                        None => {
                                            tracing::error!("connection to minecraft server not found!");
                                        }
                                    }
                                }
                                SocketPacket::ProxyDisconnect(packet) => {
                                    match self.state.lock().await.connections.get(&packet.client_id) {
                                        Some(tx) => {
                                            tx.send(ChannelMessage::Close)?;
                                        }
                                        None => {
                                            tracing::debug!("connection already closed!")
                                        }

                                    }
                                }
                                SocketPacket::ProxyPong(ping) => {
                                    let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u16;
                                    tracing::info!("pong {} ms", time.saturating_sub(ping));
                                }
                                _ => {
                                    unimplemented!("Message not implemented!")
                                }
                            }
                        }
                        // An error occurred.
                        Some(Err(e)) => {
                            tracing::error!(
                                "an error occurred while processing messages error = {:?}",
                                e
                            );
                            return Err(e.into());
                        }
                        // The stream has been exhausted.
                        None => {
                            tracing::info!("Proxy has closed the connection");
                            return Err("Proxy has closed the connection".into());
                        },
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

        Err("An error occurred".into())
    }
}
