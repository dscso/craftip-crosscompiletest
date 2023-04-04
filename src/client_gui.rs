#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod addressing;
mod client_handler;
mod client;
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
        //default_theme: Theme::Light,
        ..Default::default()
    };
    let (gui_tx, mut gui_rx) = mpsc::unbounded_channel();
    let (bck_tx, mut bck_rx) = mpsc::unbounded_channel::<GuiChangeEvent>();

    tokio::spawn(async move {
        let mut controller = gui::backend::Controller::new(gui_rx, bck_tx);
        controller.update().await;
    });

    eframe::run_native(
        "CraftIP",
        options,
        Box::new(|cc| {
            let frame = cc.egui_ctx.clone();
            gui_tx.send(GuiTriggeredEvent::FrameContext(frame)).unwrap();
            Box::new(MyApp::new(gui_tx, bck_rx))
        }),
    )
}

struct MyApp {
    login_panel: LoginPanel,
    edit_panel: EditPanel,
    loading: bool,
    servers: Vec<ServerPanel>,
    tx: GuiTriggeredChannel,
    rx: UnboundedReceiver<GuiChangeEvent>,
    frames_rendered: usize,
}

impl MyApp {
    fn new(tx: GuiTriggeredChannel, rx_bg: UnboundedReceiver<GuiChangeEvent>) -> Self {
        let mut servers = vec![ServerPanel {
            state: ServerState::Disconnected,
            server: "localhost".to_string(),
            local: "127.0.0.1:25564".to_string(),
        }, ServerPanel {
            state: ServerState::Disconnected,
            server: "myserver2.craftip.net".to_string(),
            local: "localhost:25565".to_string(),
        }, ServerPanel {
            state: ServerState::Disconnected,
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
            frames_rendered: 0,
        }
    }
    fn server_set_state(&mut self, server: &str, state: ServerState) {
        self.servers.iter_mut()
            .filter(|s| s.server == server)
            .for_each(|s| s.state = state.clone());
        println!("Server state changed: {} -> {:?}", server, state);
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frames_rendered += 1;
        // update state from background thread
        if let Ok(event) = self.rx.try_recv() {
            match event {
                GuiChangeEvent::Connected(server) => {
                    tracing::info!("connected! setting state...");
                    self.server_set_state(&server.server, ServerState::Connected);
                }
                GuiChangeEvent::Disconnected(server) => {
                    self.server_set_state(&server.server, ServerState::Disconnected);
                }
                _ => {
                    println!("Unhandled event: {:?}", event);
                }
            }
        }
        //tracing::info!("Updating gui {:?}", self.servers);
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
                    ui.label(RichText::new(format!("{}", self.frames_rendered)).small());
                });
            });
            ui.separator();

            let already_connected = self.servers.iter().any(|s| s.state != ServerState::Disconnected);
            for mut server in &mut self.servers {
                let enabled = !already_connected || server.state != ServerState::Disconnected;
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
    state: ServerState,
}

impl Default for ServerPanel {
    fn default() -> Self {
        Self {
            state: ServerState::Disconnected,
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
        let configurable = self.state == ServerState::Disconnected;
        ui.group(|ui| {
            ui.set_enabled(enabled);
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
                        match self.state {
                            ServerState::Disconnected => {
                                if ui.button("🗑").clicked() {
                                    println!("delete button clicked");
                                }
                            }
                            ServerState::Connecting => {
                                ui.label("");
                                ui.label("⌛");
                            }

                            ServerState::Disconnecting => {
                                ui.label("");
                                ui.label("⌛");
                            }
                            ServerState::Connected => {
                                // leaf green color
                                ui.label(RichText::new("0 Connected").color(Color32::from_rgb(0, 204, 0)));
                                ui.label("🔌");
                            }
                        }
                    });
                });
            });
            let (btn_txt, enabled) = match self.state {
                ServerState::Disconnected => ("Connect", true),
                ServerState::Connecting => ("Connecting...", false),
                ServerState::Connected => ("Disconnect", true),
                ServerState::Disconnecting => ("Disconnecting...", false),
            };
            ui.set_enabled(enabled);
            if ui.add_sized(
                egui::vec2(ui.available_width(), 30.0),
                egui::Button::new(btn_txt),
            ).clicked() {
                match self.state {
                    ServerState::Connected => {
                        self.state = ServerState::Disconnecting;
                        tx.send(GuiTriggeredEvent::Disconnect(Server {
                            server: self.server.clone(),
                            local: self.local.clone(),
                        })).expect("failed to send disconnect event");
                    }
                    ServerState::Disconnected => {
                        self.state = ServerState::Connecting;
                        tx.send(GuiTriggeredEvent::Connect(Server {
                            server: self.server.clone(),
                            local: self.local.clone(),
                        })).expect("failed to send disconnect event");
                    }
                    _ => unreachable!("invalid state"),
                }
            }
        });
    }
}
