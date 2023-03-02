use std::{fmt, io};
use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use crate::client_handler::{ConnectionType, Protocol, SocketPacket};
use crate::datatypes::PacketError;
use crate::proxy::{ProxyHelloPacket, ProxyPacket};
use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket, MinecraftPacket};

/// An error occurred while encoding or decoding a line.
#[derive(Debug)]
pub enum PacketCodecError {
    /// The maximum line length was exceeded.
    MaxLineLengthExceeded,
    PacketCodecError(PacketError),
    /// An IO error occurred.
    Io(io::Error),
}

pub struct PacketCodec {
    max_length: usize,
    connection_type: ConnectionType,
    protocol: Protocol,
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
                let hello_packet = match MinecraftPacket::new(buf.to_vec(), true) {
                    Ok(hello_packet) => {
                        let MinecraftPacket::MCHelloPacket(pkg) = hello_packet.into();
                        buf.advance(pkg.length);
                        self.protocol = Protocol::MC(pkg.version as u32);
                        self.connection_type = ConnectionType::MCClient;
                        Some(SocketPacket::MinecraftPacket(hello_packet))
                    }
                    // wait for more data
                    Err(PacketError::TooSmall) => return Ok(None),
                    // return error to show that parser failed
                    Err(_) => None
                };

                if hello_packet.is_some() {
                    return Ok(hello_packet);
                }

                match ProxyPacket::new(buf.to_vec(), true) {
                    Ok(proxy_hello_packet) => {
                        buf.advance(proxy_hello_packet.length);
                        self.protocol = Protocol::Proxy(proxy_hello_packet.version as u32);
                        self.connection_type = ConnectionType::ProxyClient;
                        Ok(Some(SocketPacket::ProxyPacket(proxy_hello_packet)))
                    }
                    Err(PacketError::TooSmall) => Ok(None),
                    Err(e) => Err(PacketCodecError::PacketCodecError(e))
                }
            }

            // when connection is already established
            ConnectionType::MCClient => {
                let data = buf.split_to(buf.len());

                Ok(Some(SocketPacket::MinecraftPacket()))
            }
            ConnectionType::ProxyClient => {
                if let Ok(proxy_packet) = ProxyPacket::new(buf) {
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