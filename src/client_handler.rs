use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::sync::Arc;

use bytes::BytesMut;
use futures::SinkExt;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use tracing;

use crate::minecraft::MinecraftHelloPacket;
use crate::packet_codec::PacketCodec;
use crate::proxy::ProxyDataPacket;
use crate::socket_packet::SocketPacket;

pub struct Shared {
    pub clients: HashMap<SocketAddr, Tx>,
    pub servers: HashMap<String, Tx>,
}

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<BytesMut>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<BytesMut>;

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

    /// Send a `LineCodec` encoded message to every peer, except
    /// for the sender.
    /*async fn broadcast(&mut self, sender: SocketAddr, message: &str) {
        for peer in self.servers.iter_mut() {
            if *peer.0 != sender {
                let _ = peer.1.send(message.into());
            }
        }
    }*/
    async fn send_to_server(&mut self, server: String, buf: &BytesMut) {
        for peer in self.servers.iter_mut() {
            tracing::info!("MC -> Server {}", buf.len());
            if *peer.0 == server {
                let _ = peer.1.send(buf.clone());
            }
        }
    }
    async fn send_to_client(&mut self, client: String, buf: &BytesMut) {
        for peer in self.clients.iter_mut() {
            tracing::info!("Server -> MC {}", buf.len());
            //if *peer.0 == client {
            let _ = peer.1.send(buf.clone());
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

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.

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
        // Get the client socket address
        let addr = frames.get_ref().peer_addr()?;

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.

        state.lock().await.servers.insert(server.to_string(), tx);

        Ok(Client {
            frames,
            rx,
            connection_type: ConnectionType::ProxyClient,
        })
    }
}

pub async fn process_socket_connection(
    socket: TcpStream,
    addr: SocketAddr,
    state: Arc<Mutex<Shared>>,
) -> Result<(), Box<dyn Error>> {
    let max_pkg_size = 0;
    tracing::info!("new connection from: {}", addr);
    let mut frames = Framed::new(socket, PacketCodec::new(1024 * 8));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or("No first packet received")??;
    tracing::info!("received new packet: {:?}", packet);
    let mut connection = match packet {
        SocketPacket::MCHelloPacket(hello_pkg) => {
            let mut connection = Client::new_mc_client(state.clone(), frames, &hello_pkg).await?;

            let proxy_pkg = SocketPacket::ProxyDataPacket(ProxyDataPacket::from(hello_pkg));
            //println!("Sending proxy packet: {:?}", proxy_pkg.encode().unwrap());
            let pkg = BytesMut::from(&proxy_pkg.encode().unwrap().to_vec()[..]);
            //let pkg = BytesMut::from(&packet.data.clone()[..]);
            {
                state.lock().await.send_to_server("localhost".to_string(), &pkg).await;
            }


            //state.lock().await.send_to_server("localhost".to_string(), &bufmut).await;
            connection
        }
        SocketPacket::ProxyHelloPacket(hello_pkg) => {
            Client::new_proxy_client(state.clone(), frames, &hello_pkg.hostname).await?
        }
        _ => {
            tracing::error!("Unknown protocol");
            Client::new_proxy_client(state.clone(), frames, "anonymous debug").await?
            //return Ok(());
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
                // A message was received from the current user, we should
                // broadcast this message to the other users.
                Some(Ok(msg)) => {
                    match msg {
                        SocketPacket::MCDataPacket(packet) => {
                            //tracing::info!("Received minecraft packet: {:?}", packet);
                            let proxy_pkg = SocketPacket::ProxyDataPacket(ProxyDataPacket::from(packet.clone()));
                            let pkg = BytesMut::from(&proxy_pkg.encode().unwrap().to_vec()[..]);
                            println!("Proxy pkg {} original {} {:?}", packet.data.len(), pkg.len(), packet);
                            {
                                state.lock().await.send_to_server("localhost".to_string(), &pkg).await;
                            }
                        }
                        SocketPacket::ProxyDataPacket(packet) => {
                            //tracing::info!("Received ProxyDataPacket packet: {:?}", packet);
                            {
                                let pkg = BytesMut::from(&packet.data[..]);
                                state.lock().await.send_to_client("localhost".to_string(), &pkg).await;
                            }
                        }
                        packet => {
                            tracing::info!("Received proxy packet: {:?}", packet);
                        }
                    }
                }
                // An error occurred.
                Some(Err(e)) => {
                    tracing::error!(
                        "an error occurred while processing messages for error = {:?}",
                        e
                    );
                }
                // The stream has been exhausted.
                None => {
                    tracing::info!("connection closed to {addr} closed!");
                    break;
                },
            },
        }
        //frames.send("Helloaksjdlaksjdklasjdlkasjdlkasjdlsakj".to_string()).await?;
        //let peer = Peer::new(state.clone(), frames).await?;
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

pub enum ConnectionType {
    Unknown,
    MCClient,
    ProxyClient,
}