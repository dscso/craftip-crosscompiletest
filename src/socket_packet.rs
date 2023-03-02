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
