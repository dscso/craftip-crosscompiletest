use serde::{Deserialize, Serialize};

/// ProxyHelloPacket is the first packet sent by the client to the proxy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyHelloPacket {
    pub length: usize,
    pub version: i32,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyClientJoinPacket {
    pub length: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyDataPacket {
    pub length: usize,
    pub client_id: u32,
    pub data: Vec<u8>,
}
