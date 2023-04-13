use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt, TryFutureExt};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio_util::codec::Framed;

use crate::addressing::{Distributor, DistributorError, Rx};
use crate::distributor_error;
use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::packet_codec::PacketCodec;
use crate::proxy::{
    ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyDataPacket, ProxyHelloPacket,
};
use crate::socket_packet::{ChannelMessage, SocketPacket};

pub struct Shared {
    pub distributor: Distributor,
}

#[derive(Debug)]
pub struct MCClient {
    frames: Framed<TcpStream, PacketCodec>,
    rx: Rx,
    distributor: Arc<Mutex<Distributor>>,
    addr: SocketAddr,
    id: u16,
    hostname: String,
}

#[derive(Debug)]
pub struct ProxyClient {
    frames: Framed<TcpStream, PacketCodec>,
    rx: Rx,
    distributor: Arc<Mutex<Distributor>>,
    addr: SocketAddr,
    hostname: String,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub fn new() -> Self {
        Shared {
            distributor: Distributor::new(),
        }
    }
}

impl MCClient {
    /// Create a new instance of `Peer`.
    async fn new(
        distributor: Arc<Mutex<Distributor>>,
        mut frames: Framed<TcpStream, PacketCodec>,
        hello_packet: MinecraftHelloPacket,
    ) -> Result<Self, DistributorError> {
        // Get the client socket address
        let addr = frames.get_ref().peer_addr().map_err(distributor_error!("could not get peer address"))?;
        let hostname = hello_packet.hostname.clone();
        let (tx, rx) = mpsc::unbounded_channel();

        let id = distributor.lock().await.add_client(addr, &hostname, tx)?;

        tracing::info!("added client with id: {}", id);

        // telling proxy client that there is a new client

        let client_join_packet = ProxyClientJoinPacket::new(id);
        if let Err(err) = distributor
            .lock()
            .await
            .send_to_server(&hostname, SocketPacket::from(client_join_packet))
        {
            tracing::error!("could not send first packet to proxy {}", err);
            frames.get_mut().shutdown().await.map_err(distributor_error!("could not shutdown socket"))?;
        }

        let client_id = id;
        let mut packet = ProxyDataPacket::from_mc_hello_packet(&hello_packet, client_id);
        packet.client_id = client_id;
        let packet = SocketPacket::ProxyData(packet);
        if let Err(err) = distributor.lock().await.send_to_server(&hostname, packet) {
            tracing::error!("could not send first packet to proxy {}", err);
            let _ = frames.get_mut().shutdown();
        }

        Ok(MCClient {
            frames,
            rx,
            distributor,
            addr,
            id,
            hostname: hello_packet.hostname.to_string(),
        })
    }
    /// HANDLE MC CLIENT
    pub async fn handle(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            tokio::select! {
                // A message was received from a peer. Send it to the current user.
                result = self.rx.recv() => {
                    match result {
                        Some(ChannelMessage::Packet(pkg)) => {
                            self.frames.send(pkg).await?;
                        }
                        _ => break,
                    }
                }
                result = self.frames.next() => match result {
                    Some(Ok(SocketPacket::MCData(packet))) => {
                        let packet = SocketPacket::from(ProxyDataPacket::from_mc_packet(packet, self.id));
                        if let Err(err) =
                            self.distributor.lock()
                            .await
                            .send_to_server(&self.hostname, packet)
                        {
                            tracing::error!("could not send to server {}", err);
                            break;
                        }
                    }
                    // An error occurred.
                    Some(Err(e)) => {
                        tracing::error!("Error while receiving: {:?}", e);
                    }
                    // The stream has been exhausted.
                    None => {
                        tracing::info!("connection closed to {} closed!", self.addr);
                        break;
                    },
                    _ => {
                        tracing::error!("received unknown packet from client");
                    }
                },
            }
        }

        tracing::info!("removing Minecraft client {} from state", self.addr);
        let packet = SocketPacket::from(ProxyClientDisconnectPacket::new(self.id));
        if let Err(err) = self
            .distributor
            .lock()
            .await
            .send_to_server(&self.hostname, packet)
        {
            tracing::info!("could not send disconnect packet to proxy {}", err);
        }

        if let Err(e) = self.distributor.lock().await.remove_client(&self.addr) {
            tracing::info!("Error while removing mc client {}", e);
        };
        Ok(())
    }
}

impl ProxyClient {
    async fn new(
        distributor: Arc<Mutex<Distributor>>,
        frames: Framed<TcpStream, PacketCodec>,
        packet: ProxyHelloPacket,
    ) -> Result<Self, DistributorError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let addr = frames.get_ref().peer_addr().map_err(distributor_error!("could not get peer addr"))?;
        distributor.lock().await.add_server(&packet.hostname, tx)?;

        Ok(ProxyClient {
            frames,
            rx,
            addr,
            distributor,
            hostname: packet.hostname.to_string(),
        })
    }
    /// HANDLE PROXY CLIENT
    pub async fn handle(&mut self) -> Result<(), DistributorError> {
        loop {
            tokio::select! {
                result = self.rx.recv() => {
                    match result {
                        Some(ChannelMessage::Packet(pkg)) => {
                            self.frames.send(pkg).await.map_err(distributor_error!("could not send packet"))?;
                        }
                        _ => {
                            tracing::info!("connection closed by another side of unbound channel");
                            break;
                        }
                    }
                }
                result = timeout(Duration::from_secs(60), self.frames.next()) => {
                    // catching timeout error
                    let result = match result {
                        Ok(result) => result,
                        Err(e) => {
                            tracing::info!("connection to {} timed out {e}", self.addr);
                            break;
                        }
                    };
                    match result {
                        Some(Ok(packet)) => {
                            match packet {
                                SocketPacket::ProxyDisconnect(packet) => {
                                    match self.distributor.lock().await.get_client(
                                        &self.hostname,
                                        packet.client_id,
                                    ) {
                                        Ok(client) => {
                                            client.send(ChannelMessage::Close)
                                                .map_err(distributor_error!("could not send packet"))?;
                                        }
                                        // do nothing if client already disconnected
                                        Err(DistributorError::ClientNotFound) => {}
                                        res => {
                                            res.map_err(distributor_error!("could not send packet"))?;
                                            break;
                                        }
                                    }
                                }
                                SocketPacket::ProxyData(packet) => {
                                    let client_id = packet.client_id;
                                    let mc_packet = SocketPacket::MCData(MinecraftDataPacket::from(packet));
                                    let host = &self.hostname;
                                    if let Err(err) = self.distributor
                                        .lock()
                                        .await
                                        .send_to_client(host, client_id, &mc_packet) {
                                            tracing::warn!("could not send to client {}, maybe already disconnected?", err);
                                        }
                                }
                                SocketPacket::ProxyPing(packet) => {
                                    self.frames.send(SocketPacket::ProxyPong(packet)).await
                                        .map_err(distributor_error!("could not send packet"))?
                                }
                                packet => {
                                    tracing::info!("Received proxy packet: {:?}", packet);
                                }
                            }
                        }
                        // either the channel was closed or the other side closed the channel
                        _ => break
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn close_connection(&mut self) {
        tracing::info!("removing proxy client {} from state", self.hostname);
        if let Err(e) = self.distributor.lock().await.remove_server(&self.hostname) {
            tracing::error!("Error while removing proxy client {}", e);
        };
    }
}

/// This function handles the connection to one client
/// it decides if the client is a minecraft client or a proxy client
/// forwards the traffic to the other side
/// encapsulates/decapsulates the packets
pub async fn process_socket_connection(
    socket: TcpStream,
    distributor: Arc<Mutex<Distributor>>,
) -> Result<(), Box<dyn Error>> {
    let mut frames = Framed::new(socket, PacketCodec::new(1024 * 8));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or("No first packet received")??;
    match packet {
        SocketPacket::MCHello(packet) => {
            let mut client = match MCClient::new(distributor.clone(), frames, packet.clone()).await
            {
                Ok(client) => client,
                Err(DistributorError::ServerNotFound) => {
                    tracing::info!("Server not found! {}", packet.hostname);
                    return Ok(());
                }
                Err(err) => {
                    tracing::error!("could not create new client: {}", err);
                    return Err(format!("could not create new client {:?}", err).into());
                }
            };
            tracing::info!("distributor: {}", distributor.lock().await);
            client.handle().await?;
        }
        SocketPacket::ProxyHello(packet) => {
            tracing::info!("Proxy client connected for {} from {}", packet.hostname, frames.get_ref().peer_addr()?);
            let mut client = match ProxyClient::new(distributor.clone(), frames, packet).await {
                Ok(client) => client,
                Err(err) => {
                    tracing::info!("Server not found! {}", err);
                    return Ok(());
                }
            };
            match client.handle().await {
                Ok(_) => {}
                Err(DistributorError::UnknownError(e)) => {
                    tracing::error!("Unknown error: {}", e);
                }
                Err(e) => {
                    tracing::info!("{}", e);
                }
            }
            client.close_connection().await;
        }
        _ => {
            tracing::error!("Unknown protocol");
            return Err("Unknown protocol".into());
        }
    };

    Ok(())
}
