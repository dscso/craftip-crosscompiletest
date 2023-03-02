use tracing;
use crate::client_handler::Protocol;
use crate::datatypes::PacketError;

use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket, MinecraftPacket};
use crate::packet_codec::{PacketCodecError};
use crate::proxy::{ProxyHelloPacket, ProxyPacket};

#[derive(Debug)]
pub enum SocketPacket {
    MinecraftPacket(MinecraftPacket),
    ProxyPacket(ProxyPacket),
    UnknownPacket,
}

macro_rules! packet_match {
    ($socket_type:ident, $packet_type:ident, $variant:ident, $protocol_type:ident, $buffer:expr) => {
        {
        let hello_packet = $packet_type::new($buffer.clone());
        match hello_packet {
            Ok(hello_packet) => {
                let protocol = Protocol::$protocol_type(hello_packet.version as u32);
                return (Ok(Some(SocketPacket::$socket_type($socket_type::$variant(hello_packet)))), protocol);
            }
            Err(PacketError::TooSmall) => {}
            Err(PacketError::NotMatching) => {}
            Err(e) => return (Err(PacketCodecError::PacketCodecError(e)), Protocol::Unknown),
        }
        }
    };
}
impl SocketPacket {
    pub fn new_first_package(packet: Vec<u8>) -> (Result<Option<SocketPacket>, PacketCodecError>, Protocol) {
        // check if it is MC packet
        tracing::info!("checking if its a mc pkg");
        packet_match!(MinecraftPacket, MinecraftHelloPacket, MCHelloPacket, MC, packet);
        tracing::info!("its not a mc pkg");
        packet_match!(ProxyPacket, ProxyHelloPacket, HelloPacket, Proxy, packet);
        (Ok(None), Protocol::Unknown)
    }
    /// gigantic match statement to determine the packet type
    pub fn new(buf: Vec<u8>, protocol: Protocol) -> Result<Option<SocketPacket>, PacketCodecError> {
        match protocol {
            Protocol::MC(_) => {
                let packet = MinecraftDataPacket::new(buf);
                match packet {
                    Ok(packet) => {
                        return Ok(Some(SocketPacket::MinecraftPacket(MinecraftPacket::MCDataPacket(packet))));
                    }
                    Err(PacketError::TooSmall) => {}
                    Err(e) => return Err(PacketCodecError::PacketCodecError(e)),
                }
            }
            Protocol::Proxy(_) => {
                let packet = ProxyPacket::new(buf);
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
