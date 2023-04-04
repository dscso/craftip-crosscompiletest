#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod addressing;
mod client_handler;
mod cursor;
mod datatypes;
mod gui;
mod minecraft;
mod packet_codec;
mod proxy;
mod socket_packet;

use crate::gui::gui_elements::popup;
use crate::gui::login::LoginPanel;
use eframe::egui::{CentralPanel, Color32, Layout, RichText, Ui, Window};
use eframe::emath::Align;
use eframe::{egui, Theme};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use crate::gui::gui_channel::{GuiChangeEvent, GuiTriggeredChannel, GuiTriggeredEvent, Server, ServerState};

#[tokio::main]
pub async fn main() -> Result<(), eframe::Error> {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(false)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(400.0, 600.0)),
        default_theme: Theme::Light,
        ..Default::default()
    };
    let (tx_gui, mut rx_gui) = mpsc::unbounded_channel();
    let (tx_bg, mut rx_bg) = mpsc::unbounded_channel::<GuiChangeEvent>();
    tokio::spawn(async move {
        while let Some(event) = rx_gui.recv().await {
            match event {
                GuiTriggeredEvent::Connect(server) => {
                    // sleep async 1 sec
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    tx_bg.send(GuiChangeEvent::Connected(server.clone())).unwrap();
                    tracing::info!("Connecting to server: {:?}", server);
                }
                _ => {
                    println!("Unhandled event: {:?}", event);
                }
            }
        }
    });

    eframe::run_native(
        "CraftIP",
        options,
        Box::new(|_cc| Box::new(MyApp::new(tx_gui, rx_bg))),
    )
}

struct MyApp {
    login_panel: LoginPanel,
    edit_panel: EditPanel,
    loading: bool,
    servers: Vec<ServerPanel>,
    tx: GuiTriggeredChannel,
    rx: UnboundedReceiver<GuiChangeEvent>,
}

impl MyApp {
    fn new(tx: GuiTriggeredChannel, rx_bg: UnboundedReceiver<GuiChangeEvent>) -> Self {
        let mut servers = vec![ServerPanel {
            connected: ServerState::Disconnected,
            server: "myserver.craftIP.net".to_string(),
            local: "localhost:25565".to_string(),
        }, ServerPanel {
            connected: ServerState::Disconnected,
            server: "myserver2.craftip.net".to_string(),
            local: "localhost:25565".to_string(),
        }, ServerPanel {
            connected: ServerState::Disconnected,
            server: "myserver3.craftip.net".to_string(),
            local: "localhost:25565".to_string(),
        }];
        Self {
            tx,
            rx: rx_bg,
            login_panel: LoginPanel::default(),
            edit_panel: EditPanel::default(),
            loading: false,
            servers: servers,

        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        //ctx.request_repaint();
        // update state from background thread
        if let Ok(event) = self.rx.try_recv() {
            match event {
                GuiChangeEvent::Connected(server) => {
                    self.servers.iter_mut()
                        .filter(|s| s.server == server.server)
                        .for_each(|s| s.connected = ServerState::Connected);
                }
                _ => {
                    println!("Unhandled event: {:?}", event);
                }
            }
        }
        // draw ui
        CentralPanel::default().show(ctx, |ui| {
            ui.set_enabled(!self.login_panel.open);
            self.login_panel.update_login(ctx);
            self.edit_panel.update(ctx);
            egui::menu::bar(ui, |ui| {
                ui.heading("CraftIP");
                if self.loading {
                    ui.spinner();
                }
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    ui.label(RichText::new("pre alpha").color(Color32::RED).small());
                });
            });
            ui.separator();

            let busy = self.servers.iter().any(|s| s.connected != ServerState::Disconnected);
            for mut server in &mut self.servers {
                let enabled = !busy || server.connected != ServerState::Disconnected;
                server.update(ui, &mut self.tx, enabled);
            }

            if ui.button("+").clicked() {
                println!("add button clicked");
            }
        });
    }
}

#[derive(Debug, Clone)]
struct ServerPanel {
    server: String,
    local: String,
    connected: ServerState,
}

impl Default for ServerPanel {
    fn default() -> Self {
        Self {
            connected: ServerState::Disconnected,
            server: String::new(),
            local: String::new(),
        }
    }
}

struct EditPanel {
    open: bool,
    server: String,
    local: String,
}

// implement default for LoginPanel
impl Default for EditPanel {
    fn default() -> Self {
        Self {
            open: true,
            server: String::new(),
            local: String::new(),
        }
    }
}

impl EditPanel {
    fn update(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        popup(ctx, "Edit", &mut self.open, |ui| {
            ui.label("Enter local server IP:");
            ui.add(egui::TextEdit::singleline(&mut self.local));
        });
    }
}

impl ServerPanel {
    fn update(&mut self, ui: &mut Ui, tx: &mut GuiTriggeredChannel, enabled: bool) {
        let configurable = self.connected == ServerState::Disconnected;
        ui.group(|ui| {
            ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                egui::Grid::new(self.server.as_str())
                    .num_columns(2)
                    .spacing([40.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Server IP");
                        ui.horizontal(|ui| {
                            ui.label(&self.server);
                            // copy button
                            if ui.button("📋").clicked() {
                                ui.output_mut(|o| o.copied_text = self.server.clone());
                            }
                        });
                        ui.end_row();
                        ui.label("local server");

                        ui.horizontal(|ui| {
                            ui.label(&self.local);
                            ui.set_enabled(configurable);
                            if ui.button("🖊").clicked() {
                                println!("edit button clicked");
                            }
                        });


                        ui.end_row();
                    });

                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                        ui.set_enabled(configurable);
                        if ui.button("🗑").clicked() {
                            println!("delete button clicked");
                        }
                    });
                });
            });
            let btn_txt = match self.connected {
                ServerState::Disconnected => "Connect",
                ServerState::Connecting => "Connecting...",
                ServerState::Connected => "Disconnect",
                ServerState::Disconnecting => "Disconnecting...",
            };
            ui.set_enabled(enabled);
            if ui.add_sized(
                egui::vec2(ui.available_width(), 30.0),
                egui::Button::new(btn_txt),
            ).clicked() {
                self.connected = ServerState::Connecting;
                tx.send(GuiTriggeredEvent::Connect(Server {
                    server: self.server.clone(),
                    local: self.local.clone(),
                })).expect("failed to send connect event");
            }
        });
    }
}
