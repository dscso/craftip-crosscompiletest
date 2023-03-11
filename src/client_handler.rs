use std::collections::HashMap;
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
use crate::addressing::{Rx, Tx};
use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::packet_codec::PacketCodec;
use crate::proxy::ProxyDataPacket;
use crate::socket_packet::SocketPacket;

pub struct Shared {
    pub clients: HashMap<SocketAddr, Tx>,
    pub servers: HashMap<String, Tx>,
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
            clients: HashMap::new(),
            servers: HashMap::new(),
        }
    }
    async fn send_to_server(&mut self, server: String, packet: &SocketPacket) {
        tracing::info!("MC -> Server {:?}", packet);
        for peer in self.servers.iter_mut() {
            if *peer.0 == server {
                let _ = peer.1.send(packet.clone());
            }
        }
    }
    async fn send_to_client(&mut self, client: String, packet: &SocketPacket) {
        for peer in self.clients.iter_mut() {
            tracing::info!("Server -> MC {:?}", packet);
            //if *peer.0 == client {
            let _ = peer.1.send(packet.clone());
            //}
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

        state.lock().await.clients.insert(addr, tx);

        Ok(Client {
            frames,
            rx,
            connection_type: ConnectionType::MCClient,
        })
    }
    async fn new_proxy_client(
        state: Arc<Mutex<Shared>>,
        frames: Framed<TcpStream, PacketCodec>,
        server: &str,
    ) -> io::Result<Client> {
        let addr = frames.get_ref().peer_addr()?;

        let (tx, rx) = mpsc::unbounded_channel();

        state.lock().await.servers.insert(server.to_string(), tx);

        Ok(Client {
            frames,
            rx,
            connection_type: ConnectionType::ProxyClient,
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
            let connection = Client::new_mc_client(state.clone(), frames, &packet).await?;
            let proxy_packet = SocketPacket::ProxyDataPacket(ProxyDataPacket::from(packet));
            {
                state
                    .lock()
                    .await
                    .send_to_server("localhost".to_string(), &proxy_packet)
                    .await;
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
                            let proxy_packet = SocketPacket::ProxyDataPacket(ProxyDataPacket::from(packet));
                            {
                                state
                                    .lock()
                                    .await
                                    .send_to_server("localhost".to_string(), &proxy_packet)
                                    .await;
                            }
                        }
                        SocketPacket::ProxyDataPacket(packet) => {
                            // todo verify if this is really a proxy
                            let mc_packet = SocketPacket::MCDataPacket(MinecraftDataPacket::from(packet));
                            {
                                state.lock().await.send_to_client("localhost".to_string(), &mc_packet).await;
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
    if let ConnectionType::MCClient = connection.connection_type {
        tracing::info!("removing Minecraft client {addr} from state");
        state.lock().await.clients.remove(&addr);
    }
    if let ConnectionType::ProxyClient = connection.connection_type {
        tracing::info!("removing Proxy {addr} from state");
        state.lock().await.servers.remove("localhost");
    }
    Ok(())
}

#[derive(Debug)]
pub enum ConnectionType {
    Unknown,
    MCClient,
    ProxyClient,
}
