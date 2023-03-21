use std::error::Error;
use std::sync::Arc;

use futures::{SinkExt, TryFutureExt};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::addressing::{Distributor, DistributorError, Rx};
use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::packet_codec::PacketCodec;
use crate::proxy::{ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyDataPacket};
use crate::socket_packet::{ChannelMessage, SocketPacket};
use tracing;

pub struct Shared {
    pub distributor: Distributor,
}

/// The state for each connected client.
struct Client {
    frames: Framed<TcpStream, PacketCodec>,
    rx: Rx,
    connection_type: ConnectionType,
}

impl Client {
    fn new(
        frames: Framed<TcpStream, PacketCodec>,
        rx: Rx,
        connection_type: ConnectionType,
    ) -> Client {
        Client {
            frames,
            rx,
            connection_type,
        }
    }
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub fn new() -> Self {
        Shared {
            distributor: Distributor::new(),
        }
    }
}

impl Client {
    /// Create a new instance of `Peer`.
    async fn new_mc_client(
        state: Arc<Mutex<Shared>>,
        frames: Framed<TcpStream, PacketCodec>,
        hello_packet: &MinecraftHelloPacket,
    ) -> Result<Client, DistributorError> {
        // Get the client socket address
        let addr = frames.get_ref().peer_addr().map_err(|e| {
            tracing::error!("could not get peer address, {}", e);
            DistributorError::UnknownError
        })?;

        let (tx, rx) = mpsc::unbounded_channel();

        let id = state
            .lock()
            .await
            .distributor
            .add_client(addr, &hello_packet.hostname, tx)?;

        tracing::info!("added client with id: {}", id);
        Ok(Client::new(
            frames,
            rx,
            ConnectionType::MCClient(MCClient::new(id, &hello_packet.hostname)),
        ))
    }
    async fn new_proxy_client(
        state: Arc<Mutex<Shared>>,
        frames: Framed<TcpStream, PacketCodec>,
        server: &str,
    ) -> Result<Client, DistributorError> {
        let (tx, rx) = mpsc::unbounded_channel();
        state.lock().await.distributor.add_server(server, tx)?;

        Ok(Client::new(
            frames,
            rx,
            ConnectionType::ProxyClient(ProxyClient::new(server.to_string())),
        ))
    }
}

/// This function handles the connection to one client
/// it decides if the client is a minecraft client or a proxy client
/// forwards the traffic to the other side
/// encapsulates/decapsulates the packets
pub async fn process_socket_connection(
    socket: TcpStream,
    addr: SocketAddr,
    state: Arc<Mutex<Shared>>,
) -> Result<(), Box<dyn Error>> {
    tracing::info!("new connection from: {}", addr);
    let mut frames = Framed::new(socket, PacketCodec::new(1024 * 8));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or("No first packet received")??;
    tracing::info!("received new packet: {:?}", packet);
    let mut connection = match packet {
        SocketPacket::MCHelloPacket(packet) => {
            let mut connection = match Client::new_mc_client(state.clone(), frames, &packet).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("error while adding MC client! {}", e);
                    return Ok(());
                }
            };
            let hostname = packet.hostname.clone();
            let client_join_packet = ProxyClientJoinPacket {
                length: 0,
                client_id: connection.connection_type.get_mc().id,
            };
            if let Err(err) = state
                .lock()
                .await
                .distributor
                .send_to_server(&hostname, &SocketPacket::from(client_join_packet))
            {
                tracing::error!("could not send first packet to proxy {}", err);
                let _ = connection.frames.get_mut().shutdown().map_err(|e| {
                    tracing::error!("could not shutdown socket {}", e);
                });
            }

            let client_id = connection.connection_type.get_mc().id;
            let mut packet = ProxyDataPacket::from_mc_hello_packet(packet, client_id);
            packet.client_id = client_id;
            let packet = SocketPacket::ProxyDataPacket(packet);
            if let Err(err) = state
                .lock()
                .await
                .distributor
                .send_to_server(&hostname, &packet)
            {
                tracing::error!("could not send first packet to proxy {}", err);
                let _ = connection.frames.get_mut().shutdown().map_err(|e| {
                    tracing::error!("could not shutdown socket {}", e);
                });
            }

            connection
        }
        SocketPacket::ProxyHelloPacket(packet) => {
            match Client::new_proxy_client(state.clone(), frames, &packet.hostname).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("could not add new proxy! {}", e);
                    return Ok(());
                }
            }
        }
        _ => {
            tracing::error!("Unknown protocol");
            return Ok(());
        }
    };
    //let mut connection = state.lock().await.servers.get("localhost").unwrap().clone();
    tracing::info!("waiting for new packets");
    loop {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            result = connection.rx.recv() => {
                match result {
                    Some(ChannelMessage::Packet(pkg)) => {
                        tracing::info!("Sending packet to client: {:?}", pkg);
                        connection.frames.send(pkg).await?;
                    }
                    _ => {
                        // either the channel was closed or the other side closed the channel
                        tracing::info!("connection closed by another side of unbound channel");
                        break;
                    }
                }
            }
            result = connection.frames.next() => match result {
                Some(Ok(msg)) => {
                    match msg {
                        SocketPacket::MCDataPacket(packet) => {
                            let connection_config = connection.connection_type.get_mc();
                            let packet = SocketPacket::from(ProxyDataPacket::from_mc_packet(packet, connection_config.id));

                            if let Err(err) = state
                                .lock()
                                .await.distributor
                                .send_to_server(&connection_config.hostname, &packet)
                            {
                                tracing::error!("could not send to server {}", err);
                                break;
                            }
                        }
                        SocketPacket::ProxyDisconnectPacket(packet) => {
                            tracing::info!("Received proxy disconnect packet: {:?}", packet);
                            match state.lock().await.distributor.get_client(&connection.connection_type.get_proxy().hostname, packet.client_id) {
                                Ok(client) => {
                                    if let Err(e) = client.send(ChannelMessage::Close) {
                                        tracing::error!("could not send close to client {}", e);
                                        break;
                                    }
                                }
                                Err(DistributorError::ClientNotFound) =>{},
                                Err(e) => {
                                    tracing::warn!("could not disconnect client {}, {}", packet.client_id, e);
                                    break;
                                }
                            }
                        }
                        SocketPacket::ProxyDataPacket(packet) => {
                            let client_id = packet.client_id;
                            let mc_packet = SocketPacket::MCDataPacket(MinecraftDataPacket::from(packet));
                            let host = &connection.connection_type.get_proxy().hostname;
                            if let Err(err) = state.lock().await.distributor
                                .send_to_client(host, client_id, &mc_packet) {
                                tracing::error!("could not send to client {}", err);
                                break;
                            }
                        }
                        packet => {
                            tracing::info!("Received proxy packet: {:?}", packet);
                        }
                    }
                }
                // An error occurred.
                Some(Err(e)) => {
                    tracing::error!("Error while receiving: {:?}", e);
                }
                // The stream has been exhausted.
                None => {
                    tracing::info!("connection closed to {addr} closed!");
                    break;
                },
            },
        }
    }

    match &connection.connection_type {
        ConnectionType::MCClient(config) => {
            tracing::info!("removing Minecraft client {addr} from state");
            let disconnect_packet = SocketPacket::from(ProxyClientDisconnectPacket {
                length: 0,
                client_id: config.id,
            });
            if let Err(err) = state
                .lock()
                .await
                .distributor
                .send_to_server(&config.hostname, &disconnect_packet)
            {
                tracing::error!("could not send disconnect packet to proxy {}", err);
            }

            if let Err(e) = state.lock().await.distributor.remove_client(&addr) {
                tracing::error!("Error while removing mc client {}", e);
            };
        }
        ConnectionType::ProxyClient(config) => {
            tracing::info!("removing Proxy {addr} from state");
            // todo disconnect clients
            if let Err(e) = state
                .lock()
                .await
                .distributor
                .remove_server(&config.hostname)
            {
                tracing::error!("could not remove proxy: {}", e);
                return Ok(());
            }
        }
        _ => {
            unimplemented!()
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct MCClient {
    id: u16,
    hostname: String,
}

impl MCClient {
    pub fn new(id: u16, hostname: &str) -> Self {
        MCClient {
            id,
            hostname: hostname.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ProxyClient {
    hostname: String,
}

impl ProxyClient {
    pub fn new(hostname: String) -> Self {
        ProxyClient { hostname }
    }
}

#[derive(Debug)]
pub enum ConnectionType {
    Unknown,
    MCClient(MCClient),
    ProxyClient(ProxyClient),
}

impl Default for ConnectionType {
    fn default() -> Self {
        ConnectionType::Unknown
    }
}

impl ConnectionType {
    pub fn get_mc(&self) -> &MCClient {
        match self {
            ConnectionType::MCClient(e) => e,
            _ => {
                tracing::error!("not a mc client");
                panic!("not a mc client")
            }
        }
    }
    pub fn get_proxy(&self) -> &ProxyClient {
        match self {
            ConnectionType::ProxyClient(e) => e,
            _ => {
                tracing::error!("not a proxy client");
                panic!("not a proxy client")
            }
        }
    }
    pub fn send(packet: SocketPacket) {}
}
