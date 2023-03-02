use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;

#[derive(Debug)]
pub enum ProxyPacket {
    HelloPacket(ProxyHelloPacket),
    ClientJoinPacket(ProxyClientJoinPacket),
    DataPacket(ProxyDataPacket),
    UnknownPacket,
}

/// ProxyHelloPacket is the first packet sent by the client to the proxy.
#[derive(Debug)]
pub struct ProxyHelloPacket {
    pub length: usize,
    pub version: i32,
    pub hostname: String,
}

#[derive(Debug)]
pub struct ProxyClientJoinPacket {
    pub length: usize,
}

#[derive(Debug)]
pub struct ProxyDataPacket {
    pub length: usize,
    pub client_id: u32,
    pub data: Vec<u8>,
}

impl ProxyDataPacket {
    pub(crate) fn new(buf: Vec<u8>) -> Result<ProxyDataPacket, PacketError> {
        tracing::info!("ProxyDataPacket new");
        Ok(ProxyDataPacket {
            length: 0,
            client_id: 0,
            data: buf,
        })
    }
}

impl ProxyPacket {
    pub fn new(buf: Vec<u8>) -> Result<ProxyPacket, PacketError> {
        let length = buf.len();
        if length < 1 {
            return Err(PacketError::TooSmall);
        }
        Ok(ProxyPacket::HelloPacket(ProxyHelloPacket {
            length,
            version: 0,
            hostname: String::from_utf8_lossy(&buf).parse().unwrap(),
        }))
    }
}

impl ProxyHelloPacket {
    pub fn new(buf: Vec<u8>) -> Result<ProxyHelloPacket, PacketError> {
        Ok(ProxyHelloPacket {
            length: 0,
            version: 0,
            hostname: "".to_string(),
        })
    }
}