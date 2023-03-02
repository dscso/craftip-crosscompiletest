use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;

#[derive(Debug)]
pub enum ProxyPacket {
    HelloPacket(ProxyHelloPacket),
    ClientJoinPacket(ProxyClientJoinPacket),
    DataPacket(ProxyDataPacket),
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
}

impl ProxyPacket {
    pub(crate) fn new(buf: Vec<u8>, first_time: bool) -> Result<ProxyPacket, PacketError> {
        let length = buf.len();
        if length < 1 {
            return Err(PacketError::NotValid);
        }
        Ok(ProxyPacket::HelloPacket(ProxyHelloPacket { length: buf.len(), version: 1, hostname: "test".to_string() }))
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