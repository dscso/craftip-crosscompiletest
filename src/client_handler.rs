use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::{fmt, io};

use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed, LinesCodec};

use tracing;

use bytes::{Buf, BufMut, BytesMut};
use futures::SinkExt;
use tokio::io::AsyncWriteExt;
use crate::datatypes::PacketError;

use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket, MinecraftPacket};
use crate::packet_codec::{PacketCodec, PacketCodecError};
use crate::proxy::{ProxyDataPacket, ProxyHelloPacket, ProxyPacket};


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

        Ok(Client { packet, rx, connection_type: ConnectionType::MCClient })
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

        Ok(Client { packet, rx, connection_type: ConnectionType::ProxyClient })
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
    let packet = frames.next().await.ok_or(PacketError::NotValid)??;
    tracing::info!("received new packet: {:?}", packet);


    tracing::info!("waiting for new packets");
    /*loop*/ {
        /*
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            Some(msg) = connection.rx.recv() => {
                connection.packet.send(&msg).await?;
            }
            result = connection.packet.next() => match result {
                // A message was received from the current user, we should
                // broadcast this message to the other users.
                Some(Ok(msg)) => {
                    let mut state = state.lock().await;
                    let msg = format!("{:?}", msg);

                    match connection.connection_type {
                        ConnectionType::MCClient => {
                            state.send_to_server("localhost".to_string(), &msg).await;
                        }
                        ConnectionType::ProxyClient => {
                            //state.broadcast(addr, &msg).await;
                        }
                        _ => {}
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
        }*/
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


#[derive(Debug)]
pub enum SocketPacket {
    MinecraftPacket(MinecraftPacket),
    ProxyPacket(ProxyPacket),
    UnknownPacket,
}

impl SocketPacket {
    pub fn new_first_package(packet: Vec<u8>) -> (Result<Option<SocketPacket>, PacketCodecError>, Protocol) {
        // check if it is MC packet
        tracing::info!("checking if its a mc pkg");
        let hello_packet = MinecraftHelloPacket::new(packet.clone());
        match hello_packet {
            Ok(hello_packet) => {
                let protocol = Protocol::MC(hello_packet.version as u32);
                return (Ok(Some(SocketPacket::MinecraftPacket(MinecraftPacket::MCHelloPacket(hello_packet)))), protocol);
            }
            Err(PacketError::TooSmall) => {}
            Err(PacketError::NotMatching) => {}
            Err(e) => return (Err(PacketCodecError::PacketCodecError(e)), Protocol::Unknown),
        }
        tracing::info!("its not a mc pkg");
        // check if it is proxy packet
        let hello_packet = ProxyHelloPacket::new(packet);
        match hello_packet {
            Ok(hello_packet) => {
                let protocol = Protocol::Proxy(hello_packet.version as u32);
                return (Ok(Some(SocketPacket::ProxyPacket(ProxyPacket::HelloPacket(hello_packet)))), protocol);
            }
            Err(PacketError::TooSmall) => {}
            Err(e) => return (Err(PacketCodecError::PacketCodecError(e)), Protocol::Unknown),
        }
        (Ok(None), Protocol::Unknown)
    }
    /// gigantic match statement to determine the packet type
    pub fn new(packet: Vec<u8>, protocol: Protocol) -> Result<Option<SocketPacket>, PacketCodecError> {
        match protocol {
            Protocol::MC(_) => {
                let packet = MinecraftDataPacket::new(packet);
                match packet {
                    Ok(packet) => {
                        return Ok(Some(SocketPacket::MinecraftPacket(MinecraftPacket::MCDataPacket(packet))));
                    }
                    Err(PacketError::TooSmall) => {}
                    Err(e) => return Err(PacketCodecError::PacketCodecError(e)),
                }
            }
            Protocol::Proxy(_) => {
                let packet = ProxyPacket::new(packet);
                match packet {
                    Ok(packet) => {
                        return Ok(Some(SocketPacket::ProxyPacket(packet)));
                    }
                    Err(PacketError::TooSmall) => {}
                    Err(e) => return Err(PacketCodecError::PacketCodecError(e)),
                }
            }
            _ => { unimplemented!() }
        }
        return Ok(None);
    }
}
