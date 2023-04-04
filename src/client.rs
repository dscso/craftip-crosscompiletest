use std::collections::HashMap;
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use futures::SinkExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;
use crate::packet_codec::PacketCodec;
use crate::proxy;
use crate::socket_packet::{ChannelMessage, SocketPacket};


pub type Tx = mpsc::UnboundedSender<ChannelMessage<SocketPacket>>;
pub type Rx = mpsc::UnboundedReceiver<ChannelMessage<SocketPacket>>;

#[derive(Clone)]
pub struct Client {
    proxy_server: String,
    mc_server: String,
    state: Arc<Mutex<Shared>>,
}


struct Shared {
    connections: HashMap<u16, mpsc::UnboundedSender<ChannelMessage<Vec<u8>>>>,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub fn new() -> Self {
        Shared {
            connections: HashMap::new(),
        }
    }
}

impl Client {
    pub fn new(proxy_server: String, mc_server: String) -> Self {
        Client {
            proxy_server,
            mc_server,
            state: Arc::new(Mutex::new(Shared::new())),
        }
    }
}

impl Client {
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // todo good formatting
        let mut proxy_stream = TcpStream::connect(format!("{}:25565", &self.proxy_server)).await?;
        let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 4));

        let hello = SocketPacket::from(proxy::ProxyHelloPacket {
            length: 0,
            version: 123,
            hostname: self.proxy_server.clone(),
        });
        proxy.send(hello).await?;
        tracing::info!("Sent hello packet");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(Shared::new()));
        loop {
            // Read from server 1
            tokio::select! {
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
                    tracing::info!("received message from server 1: {:?}", msg);
                    match msg {
                        SocketPacket::ProxyJoinPacket(packet) => {
                            let (client_tx, client_rx) = mpsc::unbounded_channel();
                            {
                                self.state.lock().await.connections.insert(packet.client_id, client_tx);
                            }
                            let tx_clone = tx.clone();
                            let state_clone = state.clone();
                                let scope = self.clone();
                            tokio::spawn(async move {
                                if let Err(e) = scope.handle_client(tx_clone, client_rx, packet.client_id).await {
                                    panic!("An Error occured in the handle_client function: {}", e);
                                }
                            });
                        }
                        SocketPacket::ProxyDataPacket(packet) => {
                            match self.state.lock().await.connections.get(&packet.client_id) {
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

        self.state.lock().await.connections.remove(&client_id);
        Ok(())
    }
}