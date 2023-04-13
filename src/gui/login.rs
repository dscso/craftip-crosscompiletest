use eframe::egui;
use eframe::egui::{Align, Color32, Layout, RichText};

use crate::gui;
use crate::gui::gui_elements::popup;

pub struct LoginPanel {
    pub(crate) open: bool,
    email: String,
    password: String,
}

// implement default for LoginPanel
impl Default for LoginPanel {
    fn default() -> Self {
        Self {
            open: true,
            email: String::new(),
            password: String::new(),
        }
    }
}

impl LoginPanel {
    pub(crate) fn update_login(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        popup(ctx, "Login", &mut self.open, |ui| {
            // center label
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new("To create you own server, please sign in!\nIt takes 2 Minutes")
                        .color(Color32::BLUE),
                );
            });
            ui.label("Email:");

            ui.add_sized(
                egui::vec2(ui.available_width(), 0.0),
                egui::TextEdit::singleline(&mut self.email).password(false),
            );

            ui.label("Password:");

            ui.add(gui::gui_elements::password(&mut self.password));

            // Toggle the `show_plaintext` bool with a button:
            // Show the password field:
            ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                if ui.button("Login").clicked() {
                    // perform login logic here
                    println!("Login button clicked {} , {}", self.email, self.password);
                }

                if ui.button("Register").clicked() {
                    // perform register logic here
                }
                //ui.spinner();
            });
        });
    }
}