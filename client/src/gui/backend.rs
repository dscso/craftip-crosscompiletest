use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::client::{Client, ControlTx, Stats};
use crate::gui::gui_channel::GuiTriggeredEvent;
use crate::gui::gui_channel::ServerState;
use crate::GuiState;

pub struct Controller {
    pub gui_rx: UnboundedReceiver<GuiTriggeredEvent>,
    pub state: Arc<Mutex<GuiState>>,
}

impl Controller {
    pub fn new(gui_rx: UnboundedReceiver<GuiTriggeredEvent>, state: Arc<Mutex<GuiState>>) -> Self {
        Self { gui_rx, state }
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
                        Stats::Ping(_ping) => {}
                        _ => {
                            tracing::error!("Unhandled stats: {:?}", result);
                        }
                    }
                }
                event = self.gui_rx.recv() => {
                    if event.is_none() {
                        tracing::info!("GUI channel closed");
                        break;
                    }
                    let event = event.unwrap();
                    match event {
                        GuiTriggeredEvent::Connect(server) => {
                            // sleep async 1 sec
                            tracing::info!("Connecting to server: {:?}", server);

                            let (control_tx_new, mut control_rx) = mpsc::unbounded_channel();
                            control_tx = Some(control_tx_new);

                            let state = self.state.clone();
                            let mut client = Client::new(server.server.clone(), server.local.clone(), stats_tx.clone(), control_rx).await;
                            tokio::spawn(async move {
                                // connect
                                let mut err = client.connect().await;
                                if err.is_ok() {
                                    state.lock().unwrap().set_active_server(|s| {
                                        s.state = ServerState::Connected;
                                    });
                                    // handle handle connection if connection was successful
                                    err = client.handle().await;
                                }

                                state.lock().unwrap().set_active_server(|s| {
                                    if let Err(e) = err {
                                        s.error = Some(format!("Error connecting: {:?}", e));
                                    }
                                    s.state = ServerState::Disconnected;
                                });
                            });
                        }
                        GuiTriggeredEvent::Disconnect() => {
                            // sleep async 1 sec
                            if let Some(control_tx) = &control_tx {
                                control_tx.send(crate::client::Control::Disconnect).unwrap();
                            }
                        }
                        _ => {
                            println!("Unhandled event: {:?}", event);
                        }
                    }
                }
            }
        }
    }
}
