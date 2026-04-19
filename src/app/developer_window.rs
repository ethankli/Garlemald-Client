//! Developer Settings modal. Surfaces debug knobs that are useful while
//! diagnosing protocol issues (e.g. the "Now Loading" stall after character
//! creation) but that a regular player has no reason to touch.
//!
//! Keeping this in its own modal — rather than a tab inside the game settings
//! dialog — makes accidental toggles less likely and leaves room for future
//! developer-only options (DLL-proxy mode, packet capture, etc.) without
//! cluttering the primary settings flow.

use eframe::egui;

use crate::config::DeveloperPreferences;

/// `WINEDEBUG` value applied when "verbose Wine debug logging" is enabled.
/// Adds `+ws2_32` on top of the original high-signal set so wine.log
/// captures full winsock `send` / `recv` / `WSASend` / `WSARecv` payloads
/// — the load-bearing signal for diagnosing "Now Loading" protocol
/// divergence. `+relay` was tried here earlier and traces every Win32
/// call but slowed the 1.23b client so badly that character creation
/// wouldn't complete; keep it off unless you specifically need a full
/// call trace and are willing to wait several minutes per UI step.
pub const VERBOSE_WINE_DEBUG: &str =
    "err+all,+seh,+tid,+loaddll,+module,+winsock,+ws2_32";

#[derive(Debug, Clone)]
pub enum DeveloperOutcome {
    Open,
    Accepted(DeveloperPreferences),
    Cancelled,
}

pub struct DeveloperModal {
    prefs: DeveloperPreferences,
    pub open: bool,
}

impl DeveloperModal {
    pub fn new(initial: &DeveloperPreferences) -> Self {
        Self {
            prefs: initial.clone(),
            open: true,
        }
    }

    pub fn render(&mut self, ctx: &egui::Context) -> DeveloperOutcome {
        let mut outcome = DeveloperOutcome::Open;
        let mut open = self.open;
        egui::Window::new("Developer Settings")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    "These options are intended for debugging the FFXIV 1.23b \
                     client against garlemald-server. They are off by default.",
                );
                ui.add_space(6.0);
                ui.checkbox(
                    &mut self.prefs.enable_verbose_wine_debug,
                    "Enable verbose Wine debug logging (WINEDEBUG)",
                );
                ui.add_space(2.0);
                ui.small(format!(
                    "When enabled, launches ffxivgame.patched.exe with\n\
                     WINEDEBUG=\"{VERBOSE_WINE_DEBUG}\" and writes the\n\
                     expanded Wine output to <data_dir>/logs/wine.log."
                ));
                ui.small("No effect on the native Windows backend.");

                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.prefs.enable_winsock_tracing,
                    "Enable winsock tracing (ws2_32 DLL-proxy)",
                );
                ui.add_space(2.0);
                ui.small(
                    "Deploys a ws2_32.dll hijack proxy into the game folder\n\
                     that logs every send/recv/connect/closesocket call\n\
                     the 1.23b client makes. Unhooked winsock functions are\n\
                     forwarded to a renamed copy of the real DLL so normal\n\
                     networking keeps working. Log lands at\n\
                     <game_dir>/ws2_32-trace.log.\n\
                     Requires a prebuilt ws2_32-proxy DLL — the launcher\n\
                     looks under <install>/ws2_32-proxy/target/i686-pc-\n\
                     windows-gnu/release/ws2_32.dll.",
                );

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        outcome = DeveloperOutcome::Accepted(self.prefs.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = DeveloperOutcome::Cancelled;
                    }
                });
            });
        if !open {
            outcome = DeveloperOutcome::Cancelled;
        }
        self.open = matches!(outcome, DeveloperOutcome::Open);
        outcome
    }
}
