use std::path::PathBuf;

use anyhow::Result;
use eframe::egui;

use crate::config::{preferences_file_path, Preferences};
use crate::platform::Platform;
use crate::servers::ServerDefinitions;
use crate::version::{APP_NAME, APP_VERSION};

pub fn run() -> Result<()> {
    log::info!("{APP_NAME} {APP_VERSION} starting");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 440.0])
            .with_min_inner_size([480.0, 320.0])
            .with_title(format!("{APP_NAME} — {APP_VERSION}")),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(|_cc| Ok(Box::new(LauncherApp::new()))),
    )
    .map_err(|e| anyhow::anyhow!("eframe exited: {e}"))?;
    Ok(())
}

struct LauncherApp {
    prefs: Preferences,
    prefs_path: PathBuf,
    servers: ServerDefinitions,
    selected_server_name: Option<String>,
    manual_server_address: String,
    detected_install: Option<PathBuf>,
    last_message: Option<(MessageKind, String)>,
}

#[derive(Copy, Clone)]
enum MessageKind {
    Info,
    #[allow(dead_code)]
    Error,
}

impl LauncherApp {
    fn new() -> Self {
        let prefs_path = preferences_file_path().unwrap_or_else(|_| PathBuf::from("preferences.toml"));
        let prefs = Preferences::load(&prefs_path).unwrap_or_default();
        let servers = ServerDefinitions::load_default().unwrap_or_default();
        let detected_install = crate::platform::current().detect_game_install();

        let selected_server_name = if !prefs.launcher.server_name.is_empty() {
            Some(prefs.launcher.server_name.clone())
        } else {
            servers.iter().next().map(|s| s.name.clone())
        };

        let manual_server_address = prefs.launcher.server_address.clone();

        Self {
            prefs,
            prefs_path,
            servers,
            selected_server_name,
            manual_server_address,
            detected_install,
            last_message: None,
        }
    }

    fn resolved_game_location(&self) -> Option<PathBuf> {
        self.prefs
            .launcher
            .game_location
            .clone()
            .or_else(|| self.detected_install.clone())
    }

    fn save_preferences(&self) {
        if let Err(e) = self.prefs.save(&self.prefs_path) {
            log::warn!("failed to save preferences: {e}");
        }
    }

    fn set_message(&mut self, kind: MessageKind, message: impl Into<String>) {
        self.last_message = Some((kind, message.into()));
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(APP_NAME);
            ui.label(format!("Version {APP_VERSION}"));
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Server:");
                let selected_label = self
                    .selected_server_name
                    .as_deref()
                    .unwrap_or("(none)")
                    .to_string();
                egui::ComboBox::from_id_source("server-select")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for server in self.servers.iter() {
                            ui.selectable_value(
                                &mut self.selected_server_name,
                                Some(server.name.clone()),
                                format!("{} ({})", server.address, server.name),
                            );
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Custom address:");
                ui.text_edit_singleline(&mut self.manual_server_address);
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Game location:");
                let shown = self
                    .resolved_game_location()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(not set)".to_string());
                ui.monospace(shown);
            });

            if ui.button("Browse for game folder…").clicked() {
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title("Select your FFXIV 1.x install folder")
                    .pick_folder()
                {
                    self.prefs.launcher.game_location = Some(folder);
                    self.save_preferences();
                }
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Launch (TODO)").clicked() {
                    self.set_message(
                        MessageKind::Info,
                        "Login webview integration is not yet wired up.",
                    );
                }
                if ui.button("Check for updates (TODO)").clicked() {
                    self.set_message(
                        MessageKind::Info,
                        "Patcher UI is not yet wired up.",
                    );
                }
            });

            ui.separator();

            if let Some((kind, msg)) = self.last_message.clone() {
                let color = match kind {
                    MessageKind::Info => egui::Color32::LIGHT_BLUE,
                    MessageKind::Error => egui::Color32::LIGHT_RED,
                };
                ui.colored_label(color, msg);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Persist any changes the user made via the UI before shutdown.
        if let Some(name) = &self.selected_server_name {
            self.prefs.launcher.server_name = name.clone();
        }
        self.prefs.launcher.server_address = self.manual_server_address.clone();
        self.save_preferences();
    }
}
