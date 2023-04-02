use eframe::egui;

/// Password entry field with ability to toggle character hiding.
///
/// ## Example:
/// ``` ignore
/// ui.add(password_ui(&mut my_password));
/// ```
pub fn password(password: &mut String) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| {
        let state_id = ui.id().with("show_plaintext");

        let mut show_plaintext = ui.data_mut(|d| d.get_temp::<bool>(state_id).unwrap_or(false));

        let result = ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            // Toggle the `show_plaintext` bool with a button:
            let response = ui
                .add(egui::SelectableLabel::new(show_plaintext, "👁"))
                .on_hover_text("Show/hide password");

            if response.clicked() {
                show_plaintext = !show_plaintext;
            }

            // Show the password field:
            ui.add_sized(
                egui::vec2(ui.available_width(), 0.0),
                egui::TextEdit::singleline(password).password(!show_plaintext),
            );
        });

        // Store the (possibly changed) state:
        ui.data_mut(|d| d.insert_temp(state_id, show_plaintext));

        // All done! Return the interaction response so the user can check what happened
        // (hovered, clicked, …) and maybe show a tooltip:
        result.response
    }
}