use eframe::egui;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use crate::client::{Client, ControlTx};
use crate::gui::gui_channel::{GuiChangeEvent, GuiTriggeredEvent};

pub struct Controller {
    pub gui_rx: UnboundedReceiver<GuiTriggeredEvent>,
    pub bck_tx: UnboundedSender<GuiChangeEvent>,
    pub ctx: Option<egui::Context>,
}

impl Controller {
    pub fn new(gui_rx: UnboundedReceiver<GuiTriggeredEvent>, bck_tx: UnboundedSender<GuiChangeEvent>) -> Self {
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

        let (stats_tx, stats_rx) = mpsc::unbounded_channel();
        while let Some(event) = self.gui_rx.recv().await {
            match event {
                GuiTriggeredEvent::FrameContext(ctx) => {
                    self.set_ctx(ctx);
                }
                GuiTriggeredEvent::Connect(server) => {
                    // sleep async 1 sec
                    tracing::info!("Connecting to server: {:?}", server);
                    let server_info = server.clone();
                    //
                    let (control_tx_1, control_rx) = mpsc::unbounded_channel();
                    control_tx = Some(control_tx_1);
                    let mut client = Client::new(server_info.server, server_info.local, stats_tx.clone());
                    let server_shadow = server.clone();
                    let tx = self.bck_tx.clone();
                    let ctx = self.ctx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = client.connect(control_rx).await {
                            tracing::error!("Error connecting to server: {:?}", e);
                            tx.send(GuiChangeEvent::Error(format!("Error connecting to server: {:?}", e))).unwrap();
                        }
                        tx.send(GuiChangeEvent::Disconnected(server_shadow.clone())).unwrap();
                        if let Some(ctx) = ctx {
                            ctx.request_repaint();
                        }
                    });

                    self.send_to_gui(GuiChangeEvent::Connected(server.clone()));
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