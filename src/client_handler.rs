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
use crate::minecraft_versions::MCHelloPacket;

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
        hello_packet: MCHelloPacket,
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
    let mut frames = Framed::new(socket, PacketCodec::new(1000));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or(PacketError::NotValid)??;


    let mut connection: Client = match packet {
        SocketPacket::HelloPacket(hello_packet) => Client::new_mc_client(state.clone(), frames, hello_packet).await?,
        SocketPacket::HelloProxyPacket(proxy_packet) => Client::new_proxy_client(state.clone(), frames, proxy_packet).await?,
        _ => unimplemented!()
    };
    loop {
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
                None => break,
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

pub enum Protocol {
    Unknown,
    MC(u32),
    Proxy(u32),
}

pub struct PacketCodec {
    max_length: usize,
    connection_type: ConnectionType,
    protocol: Protocol,
}

impl PacketCodec {
    /// Returns a `PacketCodec` for splitting up data into packets.
    pub fn new(max_length: usize) -> PacketCodec {
        PacketCodec {
            max_length,
            connection_type: ConnectionType::Unknown,
            protocol: Protocol::Unknown,
        }
    }
}

/// An error occurred while encoding or decoding a line.
#[derive(Debug)]
pub enum PacketCodecError {
    /// The maximum line length was exceeded.
    MaxLineLengthExceeded,
    PacketError(PacketError),
    /// An IO error occurred.
    Io(io::Error),
}

impl fmt::Display for PacketCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketCodecError::MaxLineLengthExceeded => write!(f, "max line length exceeded"),
            PacketCodecError::Io(e) => write!(f, "{}", e),
            _ => {
                write!(f, "packet error")
            }
        }
    }
}

impl From<io::Error> for PacketCodecError {
    fn from(e: io::Error) -> PacketCodecError {
        PacketCodecError::Io(e)
    }
}

impl std::error::Error for PacketCodecError {}

#[derive(Debug)]
pub struct MCDataPacket {
    pub length: usize,
    pub data: Vec<u8>,
}


// todo
#[derive(Debug)]
pub struct ProxyPacket {
    pub length: usize,
    pub data: Vec<u8>,
}

impl ProxyPacket {
    fn new(buf: &mut BytesMut) -> Option<ProxyPacket> {
        let length = buf.len();
        let data = buf.to_vec();
        Some(ProxyPacket { length, data })
    }
}

#[derive(Debug)]
pub enum SocketPacket {
    HelloPacket(MCHelloPacket),
    MCData(BytesMut),
    HelloProxyPacket(String),
    ProxyPacket(ProxyPacket),
    UnknownPacket,
}

impl Decoder for PacketCodec {
    type Item = SocketPacket;
    type Error = PacketCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<SocketPacket>, PacketCodecError> {
        // otherwise decode gets called very often!
        if buf.len() < 1 {
            return Ok(None);
        }
        if buf.len() > self.max_length {
            return Err(PacketCodecError::MaxLineLengthExceeded);
        }
        return match self.connection_type {
            // first packet
            ConnectionType::Unknown => {
                let hello_packet = MCHelloPacket::new(buf.to_vec());
                match hello_packet {
                    Ok(hello_packet) => {
                        buf.advance(hello_packet.length);
                        self.protocol = Protocol::MC(hello_packet.version as u32);
                        self.connection_type = ConnectionType::MCClient;
                        Ok(Some(SocketPacket::HelloPacket(hello_packet)))
                    }
                    Err(e) => match e {
                        PacketError::TooSmall => Ok(None),
                        _ => {
                            //Err(PacketCodecError::PacketError(e))
                            self.protocol = Protocol::Proxy(1);
                            self.connection_type = ConnectionType::ProxyClient;
                            Ok(Some(SocketPacket::HelloProxyPacket("localhost".to_string())))
                        }
                    },
                }
            }
            // when connection is already established
            ConnectionType::MCClient => {
                let data = buf.split_to(buf.len());
                Ok(Some(SocketPacket::MCData(data)))
            }
            ConnectionType::ProxyClient => {
                if let Some(proxy_packet) = ProxyPacket::new(buf) {
                    buf.advance(proxy_packet.length);
                    return Ok(Some(SocketPacket::ProxyPacket(proxy_packet)));
                }
                Ok(None)
            }
        };
    }
}

impl<T> Encoder<T> for PacketCodec
    where
        T: AsRef<str>,
{
    type Error = PacketCodecError;

    fn encode(&mut self, packet: T, buf: &mut BytesMut) -> Result<(), PacketCodecError> {
        let packet = packet.as_ref();
        buf.reserve(packet.len());
        buf.put(packet.as_bytes());
        Ok(())
    }
}