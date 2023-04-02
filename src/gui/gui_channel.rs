use tokio::sync::mpsc;

pub type GuiTriggeredChannel = mpsc::UnboundedSender<GuiTriggeredEvent>;
pub type GuiChangeChannel = mpsc::UnboundedSender<GuiChangeEvent>;

#[derive(Debug, Clone)]
pub enum GuiTriggeredEvent {
    Login,
    Logout,
    Connect(Server),
    Disconnect(Server),
    Send,
    Receive,
    Error,
}

#[derive(Debug, Clone)]
pub enum GuiChangeEvent {
    Login,
    Logout,
    Connected(Server),
    Disconnected(Server),
    Send,
    Receive,
    Error,
}

#[derive(Debug, Clone)]
pub struct Server {
    pub server: String,
    pub username: String,
    pub password: String,
    pub local: String,
}