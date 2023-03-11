use std::error::Error;
use std::io;
use std::sync::Arc;

use futures::SinkExt;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use tracing;
use crate::addressing::{Distributor, Rx};
use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::packet_codec::PacketCodec;
use crate::proxy::ProxyDataPacket;
use crate::socket_packet::SocketPacket;

pub struct Shared {
    pub distributor: Distributor,
}

/// The state for each connected client.
struct Client {
    frames: Framed<TcpStream, PacketCodec>,
    rx: Rx,
    connection_type: ConnectionType,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub(crate) fn new() -> Self {
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
    ) -> io::Result<Client> {
        // Get the client socket address
        let addr = frames.get_ref().peer_addr()?;

        let (tx, rx) = mpsc::unbounded_channel();

        let id = state.lock().await.distributor.add_client(addr, &hello_packet.hostname, tx).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, e)
        })?;

        Ok(Client {
            frames,
            rx,
            connection_type: ConnectionType::MCClient(MCClient::new(id, &hello_packet.hostname)),
        })
    }
    async fn new_proxy_client(
        state: Arc<Mutex<Shared>>,
        frames: Framed<TcpStream, PacketCodec>,
        server: &str,
    ) -> io::Result<Client> {
        let addr = frames.get_ref().peer_addr()?;

        let (tx, rx) = mpsc::unbounded_channel();

        state.lock().await.distributor.add_server(server, tx).unwrap();

        Ok(Client {
            frames,
            rx,
            connection_type: ConnectionType::ProxyClient(ProxyClient::new(server.to_string())),
        })
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
            let connection = Client::new_mc_client(state.clone(), frames, &packet).await.map_err(|e| {
                tracing::error!("could'nt find server {}", e);
                e
            })?;
            let hostname = packet.hostname.clone();
            let client_id = connection.connection_type.get_mc().unwrap().id;
            let mut proxy_packet = ProxyDataPacket::from(packet);
            proxy_packet.client_id = client_id;
            let packet = SocketPacket::ProxyDataPacket(proxy_packet);
            {
                state
                    .lock()
                    .await.distributor
                    .send_to_server(&hostname, &packet);
            }

            //state.lock().await.send_to_server("localhost".to_string(), &bufmut).await;
            connection
        }
        SocketPacket::ProxyHelloPacket(hello_pkg) => {
            Client::new_proxy_client(state.clone(), frames, &hello_pkg.hostname).await?
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
            Some(pkg) = connection.rx.recv() => {
                //tracing::info!("Sending packet to client: {:?}", pkg);
                connection.frames.send(pkg).await?;
            }
            result = connection.frames.next() => match result {
                Some(Ok(msg)) => {
                    match msg {
                        SocketPacket::MCDataPacket(packet) => {
                            let connection_config = connection.connection_type.get_mc().unwrap();
                            let mut proxy_packet = ProxyDataPacket::from(packet);
                            proxy_packet.client_id = connection_config.id;
                            let packet = SocketPacket::ProxyDataPacket(proxy_packet);
                            {
                                state
                                    .lock()
                                    .await.distributor
                                    .send_to_server(&connection_config.hostname, &packet);
                            }
                        }
                        SocketPacket::ProxyDataPacket(packet) => {
                            let client_id = packet.client_id;
                            let mc_packet = SocketPacket::MCDataPacket(MinecraftDataPacket::from(packet));
                            let host = &connection.connection_type.get_proxy().unwrap().hostname;
                            {
                                state.lock().await.distributor
                                .send_to_client(host, client_id, &mc_packet);
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
    if let ConnectionType::MCClient(config) = &connection.connection_type {
        tracing::info!("removing Minecraft client {addr} from state");
        state.lock().await.distributor.remove_client(&addr);
    }
    if let ConnectionType::ProxyClient(config) = &connection.connection_type {
        tracing::info!("removing Proxy {addr} from state");
        state.lock().await.distributor.remove_server("localhost");
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
        MCClient { id, hostname: hostname.to_string() }
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
    pub fn get_mc(&self) -> Option<&MCClient> {
        match self {
            ConnectionType::MCClient(e) => Some(e),
            _ => None,
        }
    }
    pub fn get_proxy(&self) -> Option<&ProxyClient> {
        match self {
            ConnectionType::ProxyClient(e) => Some(e),
            _ => None,
        }
    }
}