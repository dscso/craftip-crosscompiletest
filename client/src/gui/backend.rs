use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::sleep;

use crate::client::{Client, ControlTx, Stats};
use crate::gui::gui_channel::GuiTriggeredEvent;
use crate::gui::gui_channel::ServerState;
use crate::{GuiState, ServerPanel};

pub struct Controller {
    pub gui_rx: UnboundedReceiver<GuiTriggeredEvent>,
    pub state: Arc<Mutex<GuiState>>,
}

impl Controller {
    pub fn new(gui_rx: UnboundedReceiver<GuiTriggeredEvent>, state: Arc<Mutex<GuiState>>) -> Self {
        Self { gui_rx, state }
    }

    pub async fn update(&mut self) {
        let servers = vec![
            ServerPanel {
                state: ServerState::Disconnected,
                server: "myserver.craftip.net".to_string(),
                connected: 0,
                local: "25564".to_string(),
                error: None,
                edit_local: None
            },
            ServerPanel {
                state: ServerState::Disconnected,
                server: "myserver2.craftip.net".to_string(),
                connected: 0,
                local: "25564".to_string(),
                error: None,
                edit_local: None
            },
            ServerPanel {
                state: ServerState::Disconnected,
                server: "myserver3.craftip.net".to_string(),
                connected: 0,
                local: "25565".to_string(),
                error: None,
                edit_local: None
            },
            ServerPanel {
                state: ServerState::Disconnected,
                server: "hi".to_string(),
                connected: 0,
                local: "localhost:25564".to_string(),
                error: None,
                edit_local: None
            },
        ];
        sleep(Duration::from_secs(2)).await;
        self.state.lock().unwrap().modify(|state| {
            state.servers = Some(servers);
        });

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
                            }).unwrap();
                        }
                        Stats::Connected => {}
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
                            tracing::info!("Connecting to server: {:?}", server);
                            let minecraft_server = if server.local.contains(':') {
                                server.local
                            } else {
                                format!("localhost:{}", server.local)
                            };

                            let (control_tx_new, control_rx) = mpsc::unbounded_channel();
                            control_tx = Some(control_tx_new);

                            let state = self.state.clone();
                            let mut client = Client::new(server.server, minecraft_server, stats_tx.clone(), control_rx).await;
                            tokio::spawn(async move {
                                // connect
                                match client.connect().await {
                                    Ok(_) => {
                                        state.lock().unwrap().set_active_server(|s| {
                                            s.state = ServerState::Connected;
                                            s.connected = 0;
                                            s.error = None;
                                        }).unwrap();
                                    }
                                    Err(e) => {
                                        tracing::error!("Error connecting: {}", e);
                                        state.lock().unwrap().set_active_server(|s| {
                                            s.error = Some(format!("Error connecting: {}", e));
                                            s.state = ServerState::Disconnected;
                                        }).unwrap();
                                        return;
                                    }
                                }

                                // handle handle connection if connection was successful
                                let err = client.handle().await;
                                state.lock().unwrap().set_active_server(|s| {
                                    if let Err(e) = err {
                                        s.error = Some(format!("Error connecting: {}", e));
                                    }
                                    s.state = ServerState::Disconnected;
                                }).unwrap();
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
