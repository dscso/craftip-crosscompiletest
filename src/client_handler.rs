use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::sync::Arc;

use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use tracing;

use crate::minecraft::MinecraftHelloPacket;
use crate::packet_codec::PacketCodec;

pub struct Shared {
    pub clients: HashMap<SocketAddr, Tx>,
    pub servers: HashMap<String, Tx>,
}

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<String>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<String>;

/// The state for each connected client.
struct Client {
    packet: Framed<TcpStream, PacketCodec>,
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
    async fn send_to_server(&mut self, server: String, message: &str) {
        for peer in self.servers.iter_mut() {
            if *peer.0 == server {
                let _ = peer.1.send(message.into());
            }
        }
    }
}

impl Client {
    /// Create a new instance of `Peer`.
    async fn new_mc_client(
        state: Arc<Mutex<Shared>>,
        packet: Framed<TcpStream, PacketCodec>,
        hello_packet: MinecraftHelloPacket,
    ) -> io::Result<Client> {
        // Get the client socket address
        let addr = packet.get_ref().peer_addr()?;

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.

        state.lock().await.clients.insert(addr, tx);

        Ok(Client {
            packet,
            rx,
            connection_type: ConnectionType::MCClient,
        })
    }
    async fn new_proxy_client(
        state: Arc<Mutex<Shared>>,
        packet: Framed<TcpStream, PacketCodec>,
        server: String,
    ) -> io::Result<Client> {
        // Get the client socket address
        let addr = packet.get_ref().peer_addr()?;

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.

        state.lock().await.servers.insert(server, tx);

        Ok(Client {
            packet,
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
    tracing::info!("new connection from: {}", addr);
    let mut frames = Framed::new(socket, PacketCodec::new(1000));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.expect("Did not get packet back")?;
    tracing::info!("received new packet: {:?}", packet);

    tracing::info!("waiting for new packets");
    loop {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            /*Some(msg) = connection.rx.recv() => {
                connection.packet.send(&msg).await?;
            }*/
            result = frames.next() => match result {
                // A message was received from the current user, we should
                // broadcast this message to the other users.
                Some(Ok(msg)) => {
                    let mut asd :u128 = 1;
                    for i in 0..100000 {
                        asd = asd.saturating_add(i);
                    }
                    println!("Received message: {:?}", msg);
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
    Ok(())
}

pub enum ConnectionType {
    Unknown,
    MCClient,
    ProxyClient,
}

#[derive(Debug, Clone)]
pub enum Protocol {
    Unknown,
    MC(u32),
    Proxy(u32),
}
