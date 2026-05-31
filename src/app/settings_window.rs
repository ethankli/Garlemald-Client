// garlemald-client — cross-platform launcher for FINAL FANTASY XIV 1.x private servers
// Copyright (C) 2026  Samuel Stegall
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
            game_location_text: initial.map(|p| p.display().to_string()).unwrap_or_default(),
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
