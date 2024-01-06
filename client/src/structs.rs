use std::io;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use shared::crypto::ServerPrivateKey;
use shared::minecraft::MinecraftDataPacket;
use shared::packet_codec::PacketCodecError;

#[derive(Debug)]
pub enum Stats {
    Connected,
    ClientsConnected(u16),
    Ping(u16),
}

#[derive(Debug)]
pub enum Control {
    Disconnect,
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Io Error: {0}")]
    Io(#[from] io::Error),
    #[error("protocol error: {0}")]
    ProtocolError(#[from] PacketCodecError),
    #[error("Proxy closed the connection")]
    ProxyClosedConnection,
    #[error("User closed the connection")]
    UserClosedConnection,
    #[error("Timeout")]
    Timeout,
    #[error("Proxy error: {0}")]
    ProxyError(String),
    #[error("Minecraft server error. Is the server running?")]
    MinecraftServerNotFound,
    #[error("Unexpected packet: {0}")]
    UnexpectedPacket(String),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub enum ClientToProxy {
    Packet(u16, MinecraftDataPacket),
    RemoveMinecraftClient(u16),
    Death(String),
}
pub type ClientToProxyRx = UnboundedReceiver<ClientToProxy>;
pub type ClientToProxyTx = UnboundedSender<ClientToProxy>;
pub type ProxyToClient = MinecraftDataPacket;
pub type ProxyToClientRx = UnboundedReceiver<ProxyToClient>;
pub type ProxyToClientTx = UnboundedSender<ProxyToClient>;
pub type ControlTx = UnboundedSender<Control>;
pub type ControlRx = UnboundedReceiver<Control>;

pub type StatsTx = UnboundedSender<Stats>;
pub type StatsRx = UnboundedReceiver<Stats>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub server: String,
    pub local: String,
    pub auth: ServerAuthentication,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerAuthentication {
    Key(ServerPrivateKey),
}

impl Server {
    pub fn new_from_key(key: ServerPrivateKey) -> Self {
        let id = key.get_public_key().get_host();
        Self {
            server: format!("{}{}", id, shared::config::KEY_SERVER_SUFFIX),
            local: "25565".to_string(),
            auth: ServerAuthentication::Key(key),
        }
    }
}