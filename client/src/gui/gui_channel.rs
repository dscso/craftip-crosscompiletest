use std::pin::Pin;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc};
use tracing_subscriber::fmt::format;
use shared::crypto::ServerPrivateKey;
use crate::{GuiState, ServerAuthentication, ServerPanel};

pub type GuiTriggeredChannel = mpsc::UnboundedSender<GuiTriggeredEvent>;

#[derive(Debug, Clone)]
pub enum GuiTriggeredEvent {
    Login,
    Logout,
    Connect(Server),
    Disconnect(),
    Send,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub server: String,
    pub local: String,
    pub auth: ServerAuthentication
}

impl From<&ServerPanel> for Server {
    fn from(server_panel: &ServerPanel) -> Self {
        Self {
            server: server_panel.server.clone(),
            local: server_panel.local.clone(),
            auth: server_panel.auth.clone()
        }
    }
}
impl Server {
    pub fn new_from_key(key: ServerPrivateKey) -> Self {
        let id = key.get_public_key().get_host();
        Self {
            server: format!("{}{}", id, shared::config::KEY_SERVER_SUFFIX),
            local: "25565".to_string(),
            auth: ServerAuthentication::Key(key)
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}
