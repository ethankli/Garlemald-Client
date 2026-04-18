//! Game-settings modal — mirrors `GameSettingsWindow.cpp`. Editable game
//! location + "Browse…" button, OK/Cancel. Rendered as an `egui::Window`
//! from the main launcher screen.

use std::path::PathBuf;

use eframe::egui;

/// Outcome of rendering the modal once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsOutcome {
    Open,
    Accepted(Option<PathBuf>),
    Cancelled,
}

pub struct SettingsModal {
    game_location_text: String,
    pub open: bool,
}

impl SettingsModal {
    pub fn new(initial: Option<&PathBuf>) -> Self {
        Self {
            game_location_text: initial
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            open: true,
        }
    }

    pub fn render(&mut self, ctx: &egui::Context) -> SettingsOutcome {
        let mut outcome = SettingsOutcome::Open;
        let mut open = self.open;
        egui::Window::new("Game Settings")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Game install location:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.game_location_text)
                            .desired_width(320.0),
                    );
                    if ui.button("Browse…").clicked() {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title("Specify FFXIV folder")
                            .pick_folder()
                        {
                            self.game_location_text = folder.display().to_string();
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        let trimmed = self.game_location_text.trim();
                        let result = if trimmed.is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(trimmed))
                        };
                        outcome = SettingsOutcome::Accepted(result);
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = SettingsOutcome::Cancelled;
                    }
                });
            });
        if !open {
            outcome = SettingsOutcome::Cancelled;
        }
        self.open = matches!(outcome, SettingsOutcome::Open);
        outcome
    }
}
