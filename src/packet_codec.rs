use crate::client_handler::Protocol;
use crate::datatypes::PacketError;
use crate::socket_packet::SocketPacket;
use bytes::{BufMut, BytesMut};
use std::io;
use thiserror::Error;
use tokio_util::codec::{Decoder, Encoder};
use crate::minecraft::MinecraftHelloPacket;
use crate::socket_packet::SocketPacket::ProxyHelloPacket;

/// An error occurred while encoding or decoding a frame
#[derive(Debug, Error)]
pub enum PacketCodecError {
    /// The maximum line length was exceeded.
    #[error("max line length exceeded")]
    MaxLineLengthExceeded,
    #[error("PacketCodecError")]
    PacketCodecError(PacketError),
    /// An IO error occurred.
    #[error("Io Error")]
    Io(io::Error),
}

impl PacketCodec {
    /// Returns a `PacketCodec` for splitting up data into packets.
    pub fn new(max_length: usize) -> PacketCodec {
        PacketCodec {
            max_length,
            protocol: Protocol::Unknown,
        }
    }
}

impl From<io::Error> for PacketCodecError {
    fn from(e: io::Error) -> PacketCodecError {
        PacketCodecError::Io(e)
    }
}

impl From<PacketError> for PacketCodecError {
    fn from(e: PacketError) -> PacketCodecError {
        PacketCodecError::PacketCodecError(e)
    }
}

pub struct PacketCodec {
    max_length: usize,
    protocol: Protocol,
}

impl Decoder for PacketCodec {
    type Item = SocketPacket;
    type Error = PacketCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<SocketPacket>, PacketCodecError> {
        // otherwise decode gets called very often!
        if buf.is_empty() {
            return Ok(None);
        }
        if buf.len() > self.max_length {
            return Err(PacketCodecError::MaxLineLengthExceeded);
        }
        return match self.protocol {
            // first packet
            Protocol::Unknown => {
                let result = SocketPacket::parse_first_package(buf);
                match result.as_ref() {
                    Ok(Some(SocketPacket::ProxyHelloPacket(pkg))) => {
                        tracing::info!("::::::::::::: Changing connection to proxy protocol version {} ::::::::::::::", pkg.version);
                        self.protocol = Protocol::Proxy(pkg.version as u32);
                    }
                    Ok(Some(SocketPacket::MCHelloPacket(pkg))) => {
                        tracing::info!("::::::::::::: Changing connection to MC protocol version {} ::::::::::::::", pkg.version);
                        self.protocol = Protocol::MC(pkg.version as u32);
                    }
                    _ => {
                        self.protocol = Protocol::Unknown;
                    }
                }
                result
            }
            _ => SocketPacket::parse_packet(buf, self.protocol.clone()),
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
