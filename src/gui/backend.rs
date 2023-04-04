use std::sync::Arc;
use eframe::egui;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use crate::gui::gui_channel;
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
        while let Some(event) = self.gui_rx.recv().await {
            match event {
                GuiTriggeredEvent::FrameContext(ctx) => {
                    self.set_ctx(ctx);
                }
                GuiTriggeredEvent::Connect(server) => {
                    // sleep async 1 sec
                    tracing::info!("Connecting to server: {:?}", server);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    self.send_to_gui(GuiChangeEvent::Connected(server.clone()));
                }
                GuiTriggeredEvent::Disconnect(server) => {
                    // sleep async 1 sec
                    tracing::info!("Disconnecting from server: {:?}", server);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    self.send_to_gui(GuiChangeEvent::Disconnected(server.clone()));
                    tracing::info!("Disconnected from server: {:?}", server);
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