use crate::client_handler::Protocol;
use crate::datatypes::PacketError;
use crate::socket_packet::SocketPacket;
use bytes::{BufMut, BytesMut};
use std::{fmt, io};
use tokio_util::codec::{Decoder, Encoder};

/// An error occurred while encoding or decoding a line.
#[derive(Debug)]
pub enum PacketCodecError {
    /// The maximum line length was exceeded.
    MaxLineLengthExceeded,
    PacketCodecError(PacketError),
    /// An IO error occurred.
    Io(io::Error),
}

impl From<PacketError> for PacketCodecError {
    fn from(err: PacketError) -> PacketCodecError {
        PacketCodecError::PacketCodecError(err)
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
        if buf.len() < 1 {
            return Ok(None);
        }
        if buf.len() > self.max_length {
            return Err(PacketCodecError::MaxLineLengthExceeded);
        }
        return match self.protocol {
            // first packet
            Protocol::Unknown => {
                let (result, protocol) = SocketPacket::new_first_package(buf);
                self.protocol = protocol;
                result
            }
            _ => SocketPacket::new(buf, self.protocol.clone()),
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
