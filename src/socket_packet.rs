use crate::client_handler::Protocol;
use crate::datatypes::PacketError;
use bytes::BytesMut;
use tracing;

use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket, MinecraftPacket};
use crate::packet_codec::PacketCodecError;
use crate::proxy::{ProxyHelloPacket, ProxyPacket};

#[derive(Debug)]
pub enum SocketPacket {
    MinecraftPacket(MinecraftPacket),
    ProxyPacket(ProxyPacket),
    UnknownPacket,
}

impl From<MinecraftPacket> for SocketPacket {
    fn from(packet: MinecraftPacket) -> Self {
        SocketPacket::MinecraftPacket(packet)
    }
}

impl From<ProxyPacket> for SocketPacket {
    fn from(packet: ProxyPacket) -> Self {
        SocketPacket::ProxyPacket(packet)
    }
}

macro_rules! packet_match {
    ($socket_type:ident, $packet_type:ident, $variant:ident, $protocol_type:ident, $buffer:expr) => {{
        let hello_packet = $packet_type::new($buffer.inn);
        match hello_packet {
            Ok(hello_packet) => {
                let protocol = Protocol::$protocol_type(hello_packet.version as u32);
                return (
                    Ok(Some(SocketPacket::from($socket_type::$variant(
                        hello_packet,
                    )))),
                    protocol,
                );
            }
            Err(PacketError::TooSmall) => {}
            Err(PacketError::NotMatching) => {}
            Err(e) => {
                return (
                    Err(PacketCodecError::PacketCodecError(e)),
                    Protocol::Unknown,
                )
            }
        }
    }};
}
impl SocketPacket {
    pub fn new_first_package(
        packet: &mut BytesMut,
    ) -> (Result<Option<SocketPacket>, PacketCodecError>, Protocol) {
        // check if it is MC packet
        tracing::info!("checking if its a mc pkg");

        let hello_packet = MinecraftHelloPacket::new(packet);
        match hello_packet {
            Ok(hello_packet) => {
                let protocol = Protocol::MC(hello_packet.version as u32);
                return (
                    Ok(Some(SocketPacket::from(MinecraftPacket::MCHelloPacket(
                        hello_packet,
                    )))),
                    protocol,
                );
            }
            Err(PacketError::TooSmall) => {}
            Err(PacketError::NotMatching) => {}
            Err(e) => return (Err(PacketCodecError::from(e)), Protocol::Unknown),
        }
        tracing::info!("its not a mc pkg");
        //packet_match!(ProxyPacket, ProxyHelloPacket, HelloPacket, Proxy, packet);
        let hello_packet = ProxyHelloPacket::new(packet);
        match hello_packet {
            Ok(hello_packet) => {
                let protocol = Protocol::Proxy(hello_packet.version as u32);
                return (
                    Ok(Some(SocketPacket::from(ProxyPacket::HelloPacket(
                        hello_packet,
                    )))),
                    protocol,
                );
            }
            Err(PacketError::TooSmall) => {}
            Err(PacketError::NotMatching) => {}
            Err(e) => return (Err(PacketCodecError::from(e)), Protocol::Unknown),
        }
        (Ok(None), Protocol::Unknown)
    }
    /// gigantic match statement to determine the packet type
    pub fn new(
        buf: &mut BytesMut,
        protocol: Protocol,
    ) -> Result<Option<SocketPacket>, PacketCodecError> {
        println!("new packet: {:?}", protocol);
        match protocol {
            Protocol::MC(_) => {
                let packet = MinecraftDataPacket::new(buf);
                match packet {
                    Ok(packet) => {
                        return Ok(Some(SocketPacket::from(MinecraftPacket::from(packet))));
                    }
                    Err(PacketError::TooSmall) => {}
                    Err(e) => return Err(PacketCodecError::from(e)),
                }
            }
            Protocol::Proxy(_) => {
                println!("proxy packet");
                let packet = ProxyPacket::new(buf);
                match packet {
                    Ok(packet) => {
                        return Ok(Some(SocketPacket::from(packet)));
                    }
                    Err(PacketError::TooSmall) => {}
                    Err(e) => return Err(PacketCodecError::from(e)),
                }
            }
            _ => {
                unimplemented!()
            }
        }
        return Ok(None);
    }
}
