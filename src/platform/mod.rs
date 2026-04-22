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
