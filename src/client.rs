use std::collections::HashMap;
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::packet_codec::PacketCodec;
use crate::proxy;
use crate::socket_packet::{ChannelMessage, SocketPacket};
use futures::SinkExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

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

struct Shared {
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
    pub async fn connect(
        &mut self,
        mut control_rx: ControlRx,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // todo good formatting
        let proxy_stream = TcpStream::connect(format!("{}:25565", &self.proxy_server)).await?;
        let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 4));

        let hello = SocketPacket::from(proxy::ProxyHelloPacket {
            length: 0,
            version: 123,
            hostname: self.proxy_server.clone(),
        });
        proxy.send(hello).await?;
        self.stats_tx.send(Stats::Connected)?;
        tracing::info!("Connected to proxy server!");
        let (tx, mut rx) = mpsc::unbounded_channel();
        loop {
            // Read from server 1
            tokio::select! {
                result = control_rx.recv() => {
                    match result {
                        Some(Control::Disconnect) => {
                            tracing::info!("Disconnecting from proxy server");
                            break;
                        }
                        None => {
                            tracing::info!("Control channel closed");
                            break;
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
                            break;
                        }
                    }
                }
                result = proxy.next() => match result {
                    Some(Ok(msg)) => {
                        match msg {
                            SocketPacket::ProxyJoinPacket(packet) => {
                                let (client_tx, client_rx) = mpsc::unbounded_channel();
                                {
                                    self.state.lock().await.add_connection(packet.client_id, client_tx);
                                }
                                let tx_clone = tx.clone();
                                let scope = self.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = scope.clone().handle_client(tx_clone, client_rx, packet.client_id).await {
                                        panic!("An Error occured in the handle_client function: {}", e);
                                    }
                                });
                            }
                            SocketPacket::ProxyDataPacket(packet) => {
                                match self.state.lock().await.get_connection(packet.client_id) {
                                    Some(tx) => {
                                        tx.send(ChannelMessage::Packet(packet.data.to_vec()))?;
                                    }
                                    None => {
                                        tracing::error!("connection to minecraft server not found!");
                                    }
                                }
                            }
                            SocketPacket::ProxyDisconnectPacket(packet) => {
                                match self.state.lock().await.connections.get(&packet.client_id) {
                                    Some(tx) => {
                                        tx.send(ChannelMessage::Close)?;
                                    }
                                    None => {
                                        tracing::debug!("connection already closed!")
                                    }

                                }
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
                    }
                    // The stream has been exhausted.
                    None => {
                        tracing::info!("Proxy has closed the connection");
                        break
                    },
                },
            }
        }

        Ok(())
    }

    async fn handle_client(
        self,
        tx: Tx,
        mut rx: mpsc::UnboundedReceiver<ChannelMessage<Vec<u8>>>,
        client_id: u16,
    ) -> Result<(), Box<dyn Error>> {
        tracing::info!("opening new client with id {}", client_id);
        // connect to server
        let mut buf = [0; 1024];
        let mut mc_server = TcpStream::connect(self.mc_server).await?;
        loop {
            tokio::select! {
                Some(pkg) = rx.recv() => {
                    //tracing::info!("Sending packet to client: {:?}", pkg);
                    match pkg {
                        ChannelMessage::Packet(data) => {
                            mc_server.write_all(&data).await?;
                        }
                        ChannelMessage::Close => {
                            break;
                        }
                    }
                }
                n = mc_server.read(&mut buf) => {
                    let n = n?;
                    if n == 0 {
                        tracing::info!("Minecraft server closed connection!");
                        break; // server 2 has closed the connection
                    }
                    tracing::debug!("recv pkg from mc srv len: {}", n);
                    // encapsulate in ProxyDataPacket
                    let packet = SocketPacket::from(proxy::ProxyDataPacket {
                        data: buf[0..n].to_vec(),
                        client_id,
                        length: n as usize,
                    });

                    tx.send(ChannelMessage::Packet(packet))?;
                }
            }
        }
        tracing::trace!("closing client connection");

        let packet = SocketPacket::from(proxy::ProxyClientDisconnectPacket {
            length: 0,
            client_id,
        });
        tx.send(ChannelMessage::Packet(packet))?;

        self.state.lock().await.remove_connection(client_id);
        Ok(())
    }
}
