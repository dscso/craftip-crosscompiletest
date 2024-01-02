use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc};
use crate::GuiState;

pub type GuiTriggeredChannel = mpsc::UnboundedSender<GuiTriggeredEvent>;

#[derive(Debug, Clone)]
pub enum GuiTriggeredEvent {
    Login,
    Logout,
    Connect(Server),
    Disconnect(),
    Send,
}

#[derive(Debug, Clone)]
pub struct Server {
    pub server: String,
    pub local: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}
