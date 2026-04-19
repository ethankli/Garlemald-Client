//! Linux platform backend. Relies on a system-installed `wine` plus a
//! user-managed prefix under `$XDG_DATA_HOME/garlemald-client/prefix`.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::config;
use crate::crypto;
use crate::launcher::{
    apply_patches_on_disk, encryption_time_patch, lobby_host_patch, GameLaunchRequest,
};
use crate::platform::wine::{
    copy_exe_for_patching, ensure_prefix_initialized, launch_ffxiv_game, monotonic_ms_since_boot,
    WineRuntime, PREFIX_FFXIV_SUBPATH,
};
use crate::platform::Platform;

pub struct LinuxPlatform;

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
        ensure_prefix_initialized(&runtime)?;

        let tick = monotonic_ms_since_boot();
        let launch_args = crypto::build_launch_arguments(&request.session_id, tick)?;

        let src_exe = request.game_dir.join("ffxivgame.exe");
        let patched_exe = request.game_dir.join("ffxivgame.patched.exe");
        copy_exe_for_patching(&src_exe, &patched_exe)?;

        let patches = vec![encryption_time_patch(), lobby_host_patch(&request.lobby_host)?];
        apply_patches_on_disk(&patched_exe, &patches)?;

        launch_ffxiv_game(
            &runtime,
            &patched_exe,
            &launch_args.encoded_argument,
            request.wine_debug_override.as_deref(),
        )?;
        Ok(())
    }
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

