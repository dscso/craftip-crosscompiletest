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
use crate::proxy::{
    ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyDataPacket, ProxyHelloPacket,
};
use crate::socket_packet::{ChannelMessage, SocketPacket};

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
        distributor: Arc<Mutex<Distributor>>,
        mut frames: Framed<TcpStream, PacketCodec>,
        hello_packet: MinecraftHelloPacket,
    ) -> Result<Client, DistributorError> {
        // Get the client socket address
        let addr = frames.get_ref().peer_addr().map_err(|e| {
            tracing::error!("could not get peer address, {}", e);
            DistributorError::UnknownError
        })?;
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
            let _ = frames.get_mut().shutdown().map_err(|e| {
                tracing::error!("could not shutdown socket {}", e);
            });
            return Err(DistributorError::UnknownError);
        }

        let client_id = id;
        let mut packet = ProxyDataPacket::from_mc_hello_packet(&hello_packet, client_id);
        packet.client_id = client_id;
        let packet = SocketPacket::ProxyData(packet);
        if let Err(err) = distributor.lock().await.send_to_server(&hostname, packet) {
            tracing::error!("could not send first packet to proxy {}", err);
            let _ = frames.get_mut().shutdown().map_err(|e| {
                tracing::error!("could not shutdown socket {}", e);
            }).await;
            return Err(DistributorError::UnknownError);
        }

        Ok(Client::new(
            frames,
            rx,
            ConnectionType::MCClient(MCClient::new(id, &hello_packet.hostname)),
        ))
    }
    async fn new_proxy_client(
        distributor: Arc<Mutex<Distributor>>,
        frames: Framed<TcpStream, PacketCodec>,
        packet: ProxyHelloPacket,
    ) -> Result<Client, DistributorError> {
        let (tx, rx) = mpsc::unbounded_channel();

        distributor.lock().await.add_server(&packet.hostname, tx)?;

        Ok(Client::new(
            frames,
            rx,
            ConnectionType::ProxyClient(ProxyClient::new(packet.hostname.to_string())),
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
    distributor: Arc<Mutex<Distributor>>,
) -> Result<(), Box<dyn Error>> {
    tracing::info!("new connection from: {}", addr);
    tracing::info!("distributor: {:?}", distributor.lock().await);
    let mut frames = Framed::new(socket, PacketCodec::new(1024 * 8));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or("No first packet received")??;
    tracing::info!("received new packet: {:?}", packet);
    let mut connection: Client = match packet {
        SocketPacket::MCHello(packet) => {
            match Client::new_mc_client(distributor.clone(), frames, packet.clone()).await {
                Ok(client) => client,
                Err(DistributorError::ServerNotFound) => {
                    tracing::info!("Server not found! {}", packet.hostname);
                    return Ok(());
                }
                Err(err) => {
                    tracing::error!("could not create new client: {}", err);
                    return Err(format!("could not create new client {:?}", err).into());
                }
            }
        }
        SocketPacket::ProxyHello(packet) => {
            Client::new_proxy_client(distributor.clone(), frames, packet).await?
        }
        _ => {
            tracing::error!("Unknown protocol");
            return Err("Unknown protocol".into());
        }
    };

    tracing::info!("waiting for new packets");
    loop {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            result = connection.rx.recv() => {
                match result {
                    Some(ChannelMessage::Packet(pkg)) => {
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
                        SocketPacket::MCData(packet) => {
                            let connection_config = connection.connection_type.get_mc();
                            let packet = SocketPacket::from(ProxyDataPacket::from_mc_packet(packet, connection_config.id));

                            if let Err(err) =
                                distributor.lock()
                                .await
                                .send_to_server(&connection_config.hostname, packet)
                            {
                                tracing::error!("could not send to server {}", err);
                                break;
                            }
                        }
                        _ => {
                            if let Err(e) = process_proxy_packet(&mut connection, &distributor, msg).await {
                                tracing::error!("Error while process_proxy_packet: {:?}", e);
                                break;
                            }
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
    // Handle disconnect of client
    match &connection.connection_type {
        ConnectionType::MCClient(config) => {
            tracing::info!("removing Minecraft client {addr} from state");
            let packet = SocketPacket::from(ProxyClientDisconnectPacket::new(config.id));
            if let Err(err) = distributor
                .lock()
                .await
                .send_to_server(&config.hostname, packet)
            {
                tracing::error!("could not send disconnect packet to proxy {}", err);
            }

            if let Err(e) = distributor.lock().await.remove_client(&addr) {
                tracing::error!("Error while removing mc client {}", e);
            };
        }
        ConnectionType::ProxyClient(config) => {
            tracing::info!("removing Proxy {addr} from state");
            if let Err(e) = distributor.lock().await.remove_server(&config.hostname) {
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

async fn process_proxy_packet(
    connection: &mut Client,
    distributor: &Arc<Mutex<Distributor>>,
    packet: SocketPacket,
) -> Result<(), DistributorError> {
    match packet {
        SocketPacket::ProxyDisconnect(packet) => {
            tracing::info!("Received proxy disconnect packet: {:?}", packet);

            match distributor.lock().await.get_client(
                &connection.connection_type.get_proxy().hostname,
                packet.client_id,
            ) {
                Ok(client) => {
                    if let Err(e) = client.send(ChannelMessage::Close) {
                        tracing::error!("could not send close to client {}", e);
                        return Err(DistributorError::UnknownError);
                    }
                }
                Err(DistributorError::ClientNotFound) => {}
                Err(e) => {
                    tracing::error!("could not get client {}", e);
                    return Err(e);
                }
            }
        }
        SocketPacket::ProxyData(packet) => {
            let client_id = packet.client_id;
            let mc_packet = SocketPacket::MCData(MinecraftDataPacket::from(packet));
            let host = &connection.connection_type.get_proxy().hostname;
            distributor
                .lock()
                .await
                .send_to_client(host, client_id, &mc_packet)?
        }
        packet => {
            tracing::info!("Received proxy packet: {:?}", packet);
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
