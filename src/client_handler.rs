use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::{fmt, io};

use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Framed, LinesCodec};

use futures::SinkExt;
use tracing;

use bytes::{Buf, BytesMut};
use std::env;
use tokio::io::AsyncWriteExt;

use crate::datatypes::{MCHelloPacket, PacketFrame, PacketError};

pub struct Shared {
    pub peers: HashMap<SocketAddr, Tx>,
}

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<String>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<String>;

/// The state for each connected client.
struct Peer {
    /// The TCP socket wrapped with the `Lines` codec, defined below.
    ///
    /// This handles sending and receiving data on the socket. When using
    /// `Lines`, we can work at the line level instead of having to manage the
    /// raw byte operations.
    lines: Framed<TcpStream, LinesCodec>,

    /// Receive half of the message channel.
    ///
    /// This is used to receive messages from peers. When a message is received
    /// off of this `Rx`, it will be written to the socket.
    rx: Rx,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub(crate) fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    /// Send a `LineCodec` encoded message to every peer, except
    /// for the sender.
    async fn broadcast(&mut self, sender: SocketAddr, message: &str) {
        for peer in self.peers.iter_mut() {
            if *peer.0 != sender {
                let _ = peer.1.send(message.into());
            }
        }
    }
}

impl Peer {
    /// Create a new instance of `Peer`.
    async fn new(
        state: Arc<Mutex<Shared>>,
        lines: Framed<TcpStream, LinesCodec>,
    ) -> io::Result<Peer> {
        // Get the client socket address
        let addr = lines.get_ref().peer_addr()?;

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.
        state.lock().await.peers.insert(addr, tx);

        Ok(Peer { lines, rx })
    }
}

pub async fn process_socket_connection(
    socket: TcpStream,
    addr: SocketAddr,
    state: Arc<Mutex<Shared>>,
) -> Result<(), Box<dyn Error>> {
    let mut frames = Framed::new(socket, PacketCodec::new(1000));
    // In a loop, read data from the socket and write the data back.
    loop {
        print!(",");
        let packet = match frames.next().await {
            Some(packet) => packet,
            None => {
                println!("connection closed");
                return Ok(());
            }
        };

        match packet {
            Ok(packet) => {
                match packet {
                    SocketPacket::HelloPacket(hello_packet) => {
                        println!("Hello packet: {:?}", hello_packet);
                        frames.get_mut().shutdown().await?;
                    }
                    _ => {
                        println!("diff packet   ");
                    }
                }
            }
            Err(e) => {
                println!("error: {:?}", e);
                return Ok(());
            }
        }
    }
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
    /// Returns a `LinesCodec` for splitting up data into lines.
    ///
    /// # Note
    ///
    /// The returned `LinesCodec` will not have an upper bound on the length
    /// of a buffered line. See the documentation for [`new_with_max_length`]
    /// for information on why this could be a potential security risk.
    ///
    /// [`new_with_max_length`]: crate::codec::LinesCodec::new_with_max_length()
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

pub struct MCDataPacket {
    pub length: usize,
    pub data: Vec<u8>,
}

impl MCDataPacket {
    fn new(buf: BytesMut) -> MCDataPacket {
        let length = buf.len();
        let data = buf.to_vec();
        MCDataPacket { length, data }
    }
}

// todo
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

pub enum SocketPacket {
    HelloPacket(MCHelloPacket),
    MCData(BytesMut),
    ProxyPacket(ProxyPacket),
    UnknownPacket,
}

impl Decoder for PacketCodec {
    type Item = SocketPacket;
    type Error = PacketCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<SocketPacket>, PacketCodecError> {
        print!(".");
        // otherwise decode gets called very often!
        if buf.len() < 1 {
            return Ok(None);
        }
        if buf.len() > self.max_length {
            return Err(PacketCodecError::MaxLineLengthExceeded);
        }
        return match self.connection_type {
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
                        _ => Err(PacketCodecError::PacketError(e)),
                    },
                }
            }
            ConnectionType::ProxyClient => {
                if let Some(proxy_packet) = ProxyPacket::new(buf) {
                    buf.advance(proxy_packet.length);
                    return Ok(Some(SocketPacket::ProxyPacket(proxy_packet)));
                }
                Ok(None)
            }
            ConnectionType::MCClient => {
                let data = buf.split_to(buf.len());
                Ok(Some(SocketPacket::MCData(data)))
            }
        };
    }
}
