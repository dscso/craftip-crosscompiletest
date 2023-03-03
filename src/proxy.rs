use crate::cursor::CustomCursor;
use crate::datatypes::PacketError;
use bytes::BytesMut;
use std::io::BufRead;

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
    pub fn new(buf: &mut BytesMut) -> Result<ProxyPacket, PacketError> {
        Ok(ProxyPacket::DataPacket(ProxyDataPacket {
            length: 1234,
            client_id: 0,
            data: buf.to_vec(),
        }))
    }
}

impl ProxyHelloPacket {
    pub fn new(buf: &mut BytesMut) -> Result<ProxyHelloPacket, PacketError> {
        let cursor = CustomCursor::new(buf.to_vec());
        let length = buf.len();
        if length < 1 {
            return Err(PacketError::TooSmall);
        }
        let line = cursor.lines().map(|l| l.unwrap()).next();
        if line.is_none() {
            return Err(PacketError::TooSmall);
        }
        Ok(ProxyHelloPacket {
            length,
            version: 123123,
            hostname: line.unwrap(),
        })
    }
}
