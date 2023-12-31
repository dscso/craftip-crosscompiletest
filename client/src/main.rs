#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use anyhow::{bail, Context, Result};
use std::sync::{Arc, Mutex};

use eframe::egui::{CentralPanel, Color32, Layout, RichText, Ui};
use eframe::emath::Align;
use eframe::{egui, Theme};
use tokio::sync::mpsc;

use crate::gui::gui_channel::{GuiTriggeredChannel, GuiTriggeredEvent, Server, ServerState};
use crate::gui::gui_elements::popup;
use crate::gui::login::LoginPanel;

mod client;
mod connection_handler;
mod gui;

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
        default_theme: Theme::Light,
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 600.0]),
        ..Default::default()
    };
    let (gui_tx, gui_rx) = mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(GuiState::new()));
    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut controller = gui::backend::Controller::new(gui_rx, state_clone);
        controller.update().await;
    });

    eframe::run_native(
        "CraftIP",
        options,
        Box::new(|cc| {
            // add context to state to redraw from other threads
            state.lock().unwrap().set_ctx(cc.egui_ctx.clone());
            Box::new(MyApp::new(gui_tx, state))
        }),
    )
}

pub struct GuiState {
    loading: bool,
    error: Option<String>,
    servers: Vec<ServerPanel>,
    ctx: Option<egui::Context>,
}

impl GuiState {
    fn new() -> Self {
        let servers = vec![
            ServerPanel {
                state: ServerState::Disconnected,
                server: "myserver.craftip.net".to_string(),
                connected: 0,
                local: "127.0.0.1:25564".to_string(),
                error: None,
            },
            ServerPanel {
                state: ServerState::Disconnected,
                server: "myserver2.craftip.net".to_string(),
                connected: 0,
                local: "localhost:25564".to_string(),
                error: None,
            },
            ServerPanel {
                state: ServerState::Disconnected,
                server: "myserver3.craftip.net".to_string(),
                connected: 0,
                local: "localhost:25565".to_string(),
                error: None,
            },
            ServerPanel {
                state: ServerState::Disconnected,
                server: "hi".to_string(),
                connected: 0,
                local: "localhost:25564".to_string(),
                error: None,
            },
        ];
        Self {
            loading: false,
            error: None,
            servers: servers.clone(),
            ctx: None,
        }
    }
    // set_active_server pass in closure the function that will be called on the active server
    fn set_active_server(&mut self, closure: impl FnOnce(&mut ServerPanel)) -> Result<()> {
        self.servers
            .iter_mut()
            .find(|s| s.state != ServerState::Disconnected)
            .map(closure)
            .context("no active server found")?;
        self.request_repaint();
        Ok(())
    }
    fn set_ctx(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }
    fn request_repaint(&mut self) {
        match &self.ctx {
            Some(ctx) => ctx.request_repaint(),
            None => tracing::warn!("No repaint context set!"),
        }
    }
}

struct MyApp {
    login_panel: LoginPanel,
    edit_panel: EditPanel,
    state: Arc<Mutex<GuiState>>,
    tx: GuiTriggeredChannel,
    frames_rendered: usize,
}

impl MyApp {
    fn new(tx: GuiTriggeredChannel, state: Arc<Mutex<GuiState>>) -> Self {
        Self {
            tx,
            login_panel: LoginPanel::default(),
            edit_panel: EditPanel::default(),
            state,
            frames_rendered: 0,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames_rendered += 1;
        let mut state = self.state.lock().unwrap();
        // draw ui
        CentralPanel::default().show(ctx, |ui| {
            ui.set_enabled(!self.login_panel.open);
            self.login_panel.update_login(ctx);
            self.edit_panel.update(ctx);
            egui::menu::bar(ui, |ui| {
                ui.heading("CraftIP");
                if state.loading {
                    ui.spinner();
                }
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    ui.label(RichText::new("pre alpha").color(Color32::RED).small());
                    ui.label(RichText::new(format!("{}", self.frames_rendered)).small());
                });
            });
            ui.separator();

            let already_connected = state
                .servers
                .iter()
                .any(|s| s.state != ServerState::Disconnected);
            for server in &mut state.servers {
                let enabled = !already_connected || server.state != ServerState::Disconnected;
                server.update(ui, &mut self.tx, enabled);
            }

            if ui.button("+").clicked() {
                println!("add button clicked");
            }
            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(Color32::RED));
                if ui.button("OK").clicked() {
                    state.error = None;
                }
            }
        });
    }
}

#[derive(Debug, Clone)]
struct ServerPanel {
    server: String,
    connected: u16,
    local: String,
    state: ServerState,
    error: Option<String>,
}

impl Default for ServerPanel {
    fn default() -> Self {
        Self {
            state: ServerState::Disconnected,
            server: String::new(),
            connected: 0,
            local: String::new(),
            error: None,
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
                                ui.label("Connecting...");
                                ui.label("⌛");
                            }

                            ServerState::Disconnecting => {
                                ui.label("Disconnecting...");
                                ui.label("⌛");
                            }
                            ServerState::Connected => {
                                // leaf green color
                                ui.label(
                                    RichText::new(format!("{} Clients", self.connected))
                                        .color(Color32::from_rgb(0, 204, 0)),
                                );
                                ui.label("🔌");
                            }
                        }
                    });
                });
            });
            let (btn_txt, enabled) = match self.state {
                ServerState::Disconnected => ("Connect", true),
                ServerState::Connecting => ("Stop connecting", true),
                ServerState::Connected => ("Disconnect", true),
                ServerState::Disconnecting => ("Disconnecting...", false),
            };
            ui.vertical(|ui| {
                // center error
                if let Some(error) = self.error.clone() {
                    ui.label(RichText::new(error).color(Color32::RED));
                }
                ui.set_enabled(enabled);
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), 30.0),
                        egui::Button::new(btn_txt),
                    )
                    .clicked()
                {
                    self.error = None;
                    match self.state {
                        ServerState::Connected | ServerState::Connecting => {
                            self.state = ServerState::Disconnecting;
                            tx.send(GuiTriggeredEvent::Disconnect())
                                .expect("failed to send disconnect event");
                        }
                        ServerState::Disconnected => {
                            self.state = ServerState::Connecting;
                            tx.send(GuiTriggeredEvent::Connect(Server {
                                server: self.server.clone(),
                                local: self.local.clone(),
                            }))
                            .expect("failed to send disconnect event");
                        }
                        _ => unreachable!("invalid state"),
                    }
                }
            });
        });
    }
}
