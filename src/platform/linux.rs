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

//! Linux platform backend. Relies on a system-installed `wine` plus a
//! user-managed prefix under `$XDG_DATA_HOME/garlemald-client/prefix`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::config;
use crate::crypto;
use crate::launcher::{
    GameLaunchRequest, apply_patches_on_disk, assert_log_patch, encryption_time_patch,
    lobby_host_patch, null_member8_write_nop_patch, null_this_guard_patch,
};
use crate::platform::Platform;
use crate::platform::dxvk;
use crate::platform::wine::{
    WineRuntime, copy_exe_for_patching, ensure_prefix_initialized, launch_ffxiv_game,
    monotonic_ms_since_boot,
};

pub struct LinuxPlatform;

impl Default for LinuxPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxPlatform {
    pub fn new() -> Self {
        Self
    }

    fn runtime_paths() -> Result<WineRuntime> {
        let data_dir = config::data_dir()?;
        let wine_bin = which_wine()?;
        let wineserver_bin = wine_bin
            .parent()
            .map(|p| p.join("wineserver"))
            .unwrap_or_else(|| PathBuf::from("wineserver"));
        Ok(WineRuntime {
            root: data_dir.clone(),
            prefix: data_dir.join("prefix"),
            wine_bin,
            wineserver_bin,
            dyld_fallback_paths: Vec::new(),
            gst_plugin_path: None,
        })
    }
}

impl Platform for LinuxPlatform {
    fn detect_game_install(&self) -> Option<PathBuf> {
        let runtime = Self::runtime_paths().ok()?;
        let managed = runtime.install_root();
        if self.is_valid_game_location(&managed) {
            Some(managed)
        } else {
            None
        }
    }

    fn launch_game(&self, request: &GameLaunchRequest) -> Result<()> {
        let runtime = Self::runtime_paths()?;
        log::info!(
            "using wine: {} ({})",
            runtime.wine_bin.display(),
            wine_version(&runtime.wine_bin).unwrap_or_else(|| "version unknown".into()),
        );
        ensure_prefix_initialized(&runtime)?;

        // Provision DXVK (Direct3D 9 → Vulkan) into the managed prefix. This is
        // best-effort: on no-Vulkan / offline / disabled it returns None and the
        // game runs on Wine's builtin wined3d. The returned fragment is merged
        // into WINEDLLOVERRIDES by launch_ffxiv_game.
        let dxvk_overrides = dxvk::ensure_dxvk(&runtime, &config::data_dir()?.join("runtime"));

        let tick = monotonic_ms_since_boot();
        let launch_args = crypto::build_launch_arguments(&request.session_id, tick)?;

        let src_exe = request.game_dir.join("ffxivgame.exe");
        let patched_exe = request.game_dir.join("ffxivgame.patched.exe");
        copy_exe_for_patching(&src_exe, &patched_exe)?;

        let patches = vec![
            encryption_time_patch(),
            lobby_host_patch(&request.lobby_host)?,
            assert_log_patch(),
            null_this_guard_patch(),
            null_member8_write_nop_patch(),
        ];
        apply_patches_on_disk(&patched_exe, &patches)?;

        launch_ffxiv_game(
            &runtime,
            &patched_exe,
            &launch_args.encoded_argument,
            request.wine_debug_override.as_deref(),
            request.enable_winsock_proxy,
            dxvk_overrides.as_deref(),
        )?;
        Ok(())
    }
}

/// Best-effort `wine --version` for logging which runtime we resolved.
fn wine_version(wine_bin: &Path) -> Option<String> {
    let out = std::process::Command::new(wine_bin)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn which_wine() -> Result<PathBuf> {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("command -v wine")
        .output()
        .context("locating wine via `command -v`")?;
    if !out.status.success() {
        return Err(anyhow!(
            "no `wine` binary in PATH — install Wine 7+ via your distro package manager"
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        return Err(anyhow!("`command -v wine` returned empty output"));
    }
    Ok(PathBuf::from(text))
}
