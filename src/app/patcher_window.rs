//! Patcher screen — mirrors `SeventhUmbral/launcher/PatcherWindow.cpp`. Two
//! progress bars (download + apply), status labels, Cancel/Close button. The
//! worker runs on a background thread; this module only polls shared state.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use eframe::egui;

use crate::patcher::manifest::PATCH_MANIFEST;
use crate::patcher::{start_patcher_worker, PatcherShared, Phase};

pub struct PatcherScreen {
    shared: Arc<PatcherShared>,
    // Worker handle retained so the thread isn't instantly joined-on-drop; we
    // let it finish in the background after cancel and ignore the result.
    _worker: JoinHandle<()>,
    cancel_pending: bool,
    last_sample: Instant,
    last_sampled_bytes: u64,
    smoothed_rate_bytes_per_sec: f64,
}

impl PatcherScreen {
    pub fn start(game_dir: PathBuf, download_dir: PathBuf) -> Self {
        let shared = PatcherShared::new();
        let worker = start_patcher_worker(shared.clone(), game_dir, download_dir);
        Self {
            shared,
            _worker: worker,
            cancel_pending: false,
            last_sample: Instant::now(),
            last_sampled_bytes: 0,
            smoothed_rate_bytes_per_sec: 0.0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.shared.phase()
    }

    pub fn is_terminal(&self) -> bool {
        self.shared.is_terminal()
    }

    /// EWMA download-rate estimate, updated roughly every 250ms.
    fn update_rate_estimate(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample).as_secs_f64();
        if elapsed < 0.25 {
            return;
        }
        let total_bytes = self.total_downloaded_bytes();
        let delta = total_bytes.saturating_sub(self.last_sampled_bytes);
        let instantaneous = delta as f64 / elapsed;
        // 0.3 weight on the new sample: responsive but not jittery.
        self.smoothed_rate_bytes_per_sec =
            0.7 * self.smoothed_rate_bytes_per_sec + 0.3 * instantaneous;
        self.last_sample = now;
        self.last_sampled_bytes = total_bytes;
    }

    fn total_downloaded_bytes(&self) -> u64 {
        let completed = self
            .shared
            .previous_completed_bytes
            .load(Ordering::Acquire);
        let current = self.shared.download.bytes();
        completed + current
    }

    /// Renders the screen. Returns `true` when the user has dismissed the
    /// screen (clicked "Close" after completion); the caller should then
    /// transition back to the main menu.
    pub fn render(&mut self, ui: &mut egui::Ui) -> bool {
        self.update_rate_estimate();

        let phase = self.phase();

        ui.heading("Game Update");
        ui.separator();

        self.render_download_section(ui, phase);
        ui.add_space(12.0);
        self.render_patch_section(ui, phase);

        ui.separator();

        let error = self.shared.error();
        let warnings = self.shared.warnings();

        if let Some(message) = &error {
            ui.colored_label(egui::Color32::LIGHT_RED, message);
        }
        if !warnings.is_empty() {
            ui.collapsing(format!("Warnings ({})", warnings.len()), |ui| {
                for w in &warnings {
                    ui.label(w);
                }
            });
        }

        ui.separator();
        self.render_button_row(ui, phase)
    }

    fn render_download_section(&self, ui: &mut egui::Ui, phase: Phase) {
        let total_bytes = self.shared.total_download_bytes.max(1);
        let downloaded = self.total_downloaded_bytes();
        let fraction = (downloaded as f64 / total_bytes as f64).clamp(0.0, 1.0);

        let status = match phase {
            Phase::Starting => "Starting download…".to_string(),
            Phase::Downloading => {
                let idx = self.shared.download_idx.load(Ordering::Acquire);
                let entry = PATCH_MANIFEST.get(idx);
                let current = self.shared.download.bytes();
                match entry {
                    Some(entry) => {
                        let name = leaf_name(entry.path);
                        format!(
                            "Downloading {name} ({}/{}) @ {}/s",
                            format_kb(current),
                            format_kb(entry.size),
                            format_kb(self.smoothed_rate_bytes_per_sec as u64),
                        )
                    }
                    None => "Downloading…".to_string(),
                }
            }
            Phase::Patching | Phase::Done => "Download complete.".to_string(),
            Phase::Error => "Download interrupted by error.".to_string(),
            Phase::Cancelled => "Download cancelled.".to_string(),
        };

        ui.label(status);
        let bar = egui::ProgressBar::new(fraction as f32)
            .text(format!("{:.0}%", fraction * 100.0))
            .desired_width(ui.available_width());
        ui.add(bar);
    }

    fn render_patch_section(&self, ui: &mut egui::Ui, phase: Phase) {
        let total = self.shared.total_patches.max(1);
        let idx = self.shared.patch_idx.load(Ordering::Acquire);
        let fraction = match phase {
            Phase::Starting | Phase::Downloading => 0.0,
            Phase::Patching => (idx as f64 / total as f64).clamp(0.0, 1.0),
            Phase::Done => 1.0,
            Phase::Error | Phase::Cancelled => (idx as f64 / total as f64).clamp(0.0, 1.0),
        };

        let status = match phase {
            Phase::Starting | Phase::Downloading => {
                "Patcher waiting for download to complete…".to_string()
            }
            Phase::Patching => PATCH_MANIFEST
                .get(idx)
                .map(|entry| format!("Applying {}…", leaf_name(entry.path)))
                .unwrap_or_else(|| "Applying patch…".to_string()),
            Phase::Done => "Complete!".to_string(),
            Phase::Error => "Patch interrupted by error.".to_string(),
            Phase::Cancelled => "Patch cancelled.".to_string(),
        };

        ui.label(status);
        let bar = egui::ProgressBar::new(fraction as f32)
            .text(format!("{:.0}%", fraction * 100.0))
            .desired_width(ui.available_width());
        ui.add(bar);
    }

    fn render_button_row(&mut self, ui: &mut egui::Ui, _phase: Phase) -> bool {
        let mut dismiss = false;
        ui.horizontal(|ui| {
            if self.is_terminal() {
                if ui.button("Close").clicked() {
                    dismiss = true;
                }
            } else if self.cancel_pending {
                ui.label("Cancelling, please wait…");
            } else if ui.button("Cancel").clicked() {
                self.shared.request_cancel();
                self.cancel_pending = true;
            }
        });
        dismiss
    }
}

fn format_kb(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else {
        format!("{:.0} KB", b / KB)
    }
}

fn leaf_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
