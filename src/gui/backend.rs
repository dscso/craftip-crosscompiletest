use std::sync::{Arc, Mutex};

use eframe::egui;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::client::{Client, ControlTx, Stats};
use crate::gui::gui_channel::GuiTriggeredEvent;
use crate::gui::gui_channel::ServerState;
use crate::GuiState;

pub struct Controller {
    pub gui_rx: UnboundedReceiver<GuiTriggeredEvent>,
    pub state: Arc<Mutex<GuiState>>,
    pub ctx: Option<egui::Context>,
}

impl Controller {
    pub fn new(gui_rx: UnboundedReceiver<GuiTriggeredEvent>, state: Arc<Mutex<GuiState>>) -> Self {
        Self {
            gui_rx,
            state,
            ctx: None,
        }
    }
    pub fn set_ctx(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }

    pub async fn update(&mut self) {
        let mut control_tx: Option<ControlTx> = None;

        let (stats_tx, mut stats_rx) = mpsc::unbounded_channel();
        loop {
            tokio::select! {
                result = stats_rx.recv() => {
                    if result.is_none() {
                        tracing::info!("Stats channel closed");
                        break;
                    }
                    let result = result.unwrap();
                    match result {
                        Stats::ClientsConnected(clients) => {
                            tracing::info!("Clients connected: {}", clients);
                            self.state.lock().unwrap().set_active_server(|s| {
                                s.connected = clients;
                            });
                        }
                        Stats::Connected => {
                            tracing::info!("Connected to server!");
                            // clean all errors
                            self.state.lock().unwrap().servers.iter_mut().for_each(|s| s.error = None);
                            // set active server to connected
                            self.state.lock().unwrap().set_active_server(|s| {
                                s.state = ServerState::Connected;
                                s.connected = 0;
                            });
                        }
                        _ => {
                            println!("Unhandled stats: {:?}", result);
                        }
                    }

                    if let Some(ctx) = &self.ctx {
                        ctx.request_repaint();
                    }
                }
                event = self.gui_rx.recv() => {
                    if event.is_none() {
                        tracing::info!("GUI channel closed");
                        break;
                    }
                    let event = event.unwrap();
                    match event {
                        GuiTriggeredEvent::FrameContext(ctx) => {
                            self.set_ctx(ctx);
                        }
                        GuiTriggeredEvent::Connect(server) => {
                            // sleep async 1 sec
                            tracing::info!("Connecting to server: {:?}", server);

                            //
                            let (control_tx_new, control_rx) = mpsc::unbounded_channel();
                            control_tx = Some(control_tx_new);

                            let server_shadow = server.clone();
                            let ctx = self.ctx.clone();
                            let stats_tx_clone = stats_tx.clone();
                            let state = self.state.clone();
                            tokio::spawn(async move {
                                let mut client = Client::new(server_shadow.server.clone(), server_shadow.local.clone(), stats_tx_clone).await;
                                if let Err(e) = client.connect(control_rx).await {
                                    tracing::error!("Error connecting to server: {:?}", e);
                                    state.lock().unwrap().set_active_server(|s| {
                                        s.error = Some(format!("Error connecting to server: {:?}", e));
                                    });
                                }
                                state.lock().unwrap().set_active_server(|s| {
                                    s.state = ServerState::Disconnected;
                                });
                                if let Some(ctx) = ctx {
                                    ctx.request_repaint();
                                }
                            });
                        }
                        GuiTriggeredEvent::Disconnect(server) => {
                            // sleep async 1 sec
                            tracing::info!("Disconnecting from server: {:?}", server);
                            if let Some(control_tx) = &control_tx {
                                control_tx.send(crate::client::Control::Disconnect).unwrap();
                            }
                        }
                        _ => {
                            println!("Unhandled event: {:?}", event);
                        }
                    }
                    if let Some(ctx) = &self.ctx {
                        ctx.request_repaint();
                    }
                }
            }
        }
    }
}
