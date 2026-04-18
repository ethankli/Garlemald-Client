//! macOS (Apple Silicon) platform backend. Manages its own Wine prefix under
//! `~/Library/Application Support/garlemald-client/`, downloading the
//! Sikarugir CrossOver engine + frameworks on first use.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::config;
use crate::crypto;
use crate::launcher::{
    apply_patches_on_disk, encryption_time_patch, lobby_host_patch, GameLaunchRequest,
};
use crate::platform::wine::{
    copy_exe_for_patching, ensure_prefix_initialized, launch_ffxiv_game, WineRuntime,
    PREFIX_FFXIV_SUBPATH,
};
use crate::platform::Platform;

const WRAPPER_VERSION: &str = "1.0.11";
const WRAPPER_URL: &str =
    "https://github.com/Sikarugir-App/Wrapper/releases/download/v1.0/Template-1.0.11.tar.xz";
const ENGINE_NAME: &str = "WS12WineCX24.0.7_7";
const ENGINE_URL: &str =
    "https://github.com/Sikarugir-App/Engines/releases/download/v1.0/WS12WineCX24.0.7_7.tar.xz";

pub struct MacosPlatform;

impl MacosPlatform {
    pub fn new() -> Self {
        Self
    }

    fn runtime_paths() -> Result<WineRuntime> {
        let data_dir = config::data_dir()?;
        let root = data_dir.clone();
        let runtime_root = root.join("runtime");
        let wine_bin = runtime_root.join("wswine.bundle/bin/wine");
        let wineserver_bin = runtime_root.join("wswine.bundle/bin/wineserver");
        let dyld_fallback_paths = vec![
            runtime_root.join("Frameworks"),
            runtime_root.join("wswine.bundle/lib"),
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/usr/lib"),
        ];
        Ok(WineRuntime {
            root,
            prefix: data_dir.join("prefix"),
            wine_bin,
            wineserver_bin,
            dyld_fallback_paths,
        })
    }
}

impl Platform for MacosPlatform {
    fn detect_game_install(&self) -> Option<PathBuf> {
        // First, our own managed prefix.
        if let Ok(runtime) = Self::runtime_paths() {
            let managed = runtime.install_root();
            if self.is_valid_game_location(&managed) {
                return Some(managed);
            }
        }
        // Fall back to the xiv1point0-apple-silicon-installer layout if the
        // user ran that installer separately.
        if let Some(home) = std::env::var_os("HOME") {
            let guess = PathBuf::from(home)
                .join("Documents/Programming/server-workspace/xiv1point0-apple-silicon-installer/target/prefix")
                .join(PREFIX_FFXIV_SUBPATH);
            if self.is_valid_game_location(&guess) {
                return Some(guess);
            }
        }
        None
    }

    fn launch_game(&self, request: &GameLaunchRequest) -> Result<()> {
        let runtime = Self::runtime_paths()?;
        ensure_runtime_downloaded(&runtime)?;
        ensure_prefix_initialized(&runtime)?;

        let tick = current_tick_count_ms();
        let launch_args = crypto::build_launch_arguments(&request.session_id, tick)?;

        let src_exe = request.game_dir.join("ffxivgame.exe");
        let patched_exe = request.game_dir.join("ffxivgame.patched.exe");
        copy_exe_for_patching(&src_exe, &patched_exe)?;

        let patches = vec![encryption_time_patch(), lobby_host_patch(&request.lobby_host)?];
        apply_patches_on_disk(&patched_exe, &patches)?;

        launch_ffxiv_game(&runtime, &patched_exe, &launch_args.encoded_argument)?;
        Ok(())
    }
}

fn ensure_runtime_downloaded(runtime: &WineRuntime) -> Result<()> {
    let frameworks = runtime.root.join("runtime/Frameworks");
    let wswine_bin = runtime.root.join("runtime/wswine.bundle/bin/wine");

    fs::create_dir_all(runtime.root.join("runtime"))
        .with_context(|| format!("creating runtime dir {}", runtime.root.display()))?;

    if !frameworks.exists() {
        log::info!("downloading Sikarugir wrapper {WRAPPER_VERSION}");
        let tmp = tempfile::tempdir().context("creating tmp dir for wrapper")?;
        let archive = tmp.path().join("wrapper.tar.xz");
        download_to(WRAPPER_URL, &archive)?;
        extract_tar_xz(&archive, tmp.path())?;
        let src = tmp
            .path()
            .join(format!("Template-{WRAPPER_VERSION}.app/Contents/Frameworks"));
        copy_dir_all(&src, &frameworks).context("copying Frameworks")?;
    }

    if !wswine_bin.exists() {
        log::info!("downloading Wine engine {ENGINE_NAME}");
        let tmp = tempfile::tempdir().context("creating tmp dir for engine")?;
        let archive = tmp.path().join("engine.tar.xz");
        download_to(ENGINE_URL, &archive)?;
        extract_tar_xz(&archive, tmp.path())?;
        let src = tmp.path().join("wswine.bundle");
        let dest = runtime.root.join("runtime/wswine.bundle");
        if dest.exists() {
            fs::remove_dir_all(&dest).context("removing stale wswine.bundle")?;
        }
        fs::rename(&src, &dest)
            .or_else(|_| copy_dir_all(&src, &dest))
            .context("installing wswine.bundle")?;
    }

    Ok(())
}

fn download_to(url: &str, dst: &Path) -> Result<()> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut reader = response.into_reader();
    let mut out = fs::File::create(dst)
        .with_context(|| format!("creating {}", dst.display()))?;
    std::io::copy(&mut reader, &mut out)?;
    Ok(())
}

fn extract_tar_xz(archive: &Path, dst: &Path) -> Result<()> {
    // Use the system `tar` to avoid pulling an xz decoder into the Rust build.
    let status = std::process::Command::new("tar")
        .arg("-xJf")
        .arg(archive)
        .arg("-C")
        .arg(dst)
        .status()
        .context("running tar -xJf")?;
    if !status.success() {
        return Err(anyhow!("tar -xJf failed with {status:?}"));
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else if ty.is_symlink() {
            let link_target = fs::read_link(entry.path())?;
            let _ = std::os::unix::fs::symlink(&link_target, &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

fn current_tick_count_ms() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    (dur.as_millis() as u32).wrapping_sub(0)
}
