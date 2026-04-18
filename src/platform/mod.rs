use std::path::PathBuf;

use anyhow::Result;

use crate::launcher::GameLaunchRequest;

/// Platform backend that abstracts game detection + launch across Windows,
/// macOS, and Linux. Each target compiles exactly one implementation.
pub trait Platform: Send + Sync {
    /// Attempts to detect an existing FFXIV 1.x install on this system.
    fn detect_game_install(&self) -> Option<PathBuf>;

    /// Returns `true` when the directory looks like a valid FFXIV install
    /// (contains `ffxivboot.exe`).
    fn is_valid_game_location(&self, path: &std::path::Path) -> bool {
        path.join("ffxivboot.exe").exists()
    }

    /// Launches the game and applies the two memory patches.
    fn launch_game(&self, request: &GameLaunchRequest) -> Result<()>;
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsPlatform as ActivePlatform;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosPlatform as ActivePlatform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxPlatform as ActivePlatform;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn current() -> ActivePlatform {
    ActivePlatform::new()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod wine;
