use eframe::egui;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::client::{Client, ControlTx, Stats};
use crate::gui::gui_channel::{GuiChangeEvent, GuiTriggeredEvent};

pub struct Controller {
    pub gui_rx: UnboundedReceiver<GuiTriggeredEvent>,
    pub bck_tx: UnboundedSender<GuiChangeEvent>,
    pub ctx: Option<egui::Context>,
}

impl Controller {
    pub fn new(
        gui_rx: UnboundedReceiver<GuiTriggeredEvent>,
        bck_tx: UnboundedSender<GuiChangeEvent>,
    ) -> Self {
        Self {
            gui_rx,
            bck_tx,
            ctx: None,
        }
    }
    pub fn set_ctx(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }
    pub fn send_to_gui(&mut self, event: GuiChangeEvent) {
        self.bck_tx.send(event).unwrap();
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
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
                            self.send_to_gui(GuiChangeEvent::Stats(clients));
                        }
                        Stats::Connected => {
                            self.send_to_gui(GuiChangeEvent::Connected);
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
                            let tx = self.bck_tx.clone();
                            let ctx = self.ctx.clone();
                            let stats_tx_clone = stats_tx.clone();
                            tokio::spawn(async move {
                                let mut client = Client::new(server_shadow.server.clone(), server_shadow.local.clone(), stats_tx_clone).await;
                                if let Err(e) = client.connect(control_rx).await {
                                    tracing::error!("Error connecting to server: {:?}", e);
                                    tx.send(GuiChangeEvent::Error(format!("Error connecting to server: {:?}", e))).unwrap();
                                }
                                tx.send(GuiChangeEvent::Disconnected).unwrap();
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
