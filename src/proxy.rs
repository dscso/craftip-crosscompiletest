use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;
use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use std::io::BufRead;

#[derive(Serialize, Deserialize, Debug)]
pub enum ProxyPacket {
    HelloPacket(ProxyHelloPacket),
    ClientJoinPacket(ProxyClientJoinPacket),
    DataPacket(ProxyDataPacket),
    UnknownPacket,
}

/// ProxyHelloPacket is the first packet sent by the client to the proxy.
#[derive(Serialize, Deserialize, Debug)]
pub struct ProxyHelloPacket {
    pub length: usize,
    pub version: i32,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProxyClientJoinPacket {
    pub length: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProxyDataPacket {
    pub length: usize,
    pub client_id: u32,
    pub data: Vec<u8>,
}

impl ProxyPacket {
    pub fn decode(buf: &mut BytesMut) -> Result<ProxyPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        // create new empty string
        let mut line = String::new();
        // read a line into the string if there is one advance buffer if not return error
        if cursor.read_line(&mut line).is_err() {
            return Err(PacketError::TooSmall);
        }
        buf.advance(cursor.position() as usize);
        let packet: ProxyPacket = serde_json::from_str(&line).map_err(|_| PacketError::NotValid)?;
        Ok(packet)
    }
    pub fn encode(&self) -> Result<Vec<u8>, PacketError> {
        let packet = serde_json::to_string(self).map_err(|_| PacketError::NotValid)?;
        Ok(packet.as_bytes().to_vec())
    }
}
