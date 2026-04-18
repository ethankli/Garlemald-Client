//! Shared Wine-prefix plumbing used by the macOS and Linux platform backends.
//!
//! A managed "runtime" looks like:
//!
//! ```text
//! <data_dir>/garlemald-client/
//! ├── prefix/                              # WINEPREFIX
//! │   └── drive_c/Program Files (x86)/SquareEnix/FINAL FANTASY XIV/
//! └── runtime/                             # macOS only
//!     ├── wswine.bundle/                   # Sikarugir CrossOver engine
//!     └── Frameworks/                      # bundled MoltenVK, libinotify, …
//! ```
//!
//! On Linux we rely on a system-installed `wine`; the `runtime/` directory is
//! a no-op there.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

/// Relative path inside the prefix to the FFXIV install root, matching the
/// default the InstallShield installer uses.
pub const PREFIX_FFXIV_SUBPATH: &str = "drive_c/Program Files (x86)/SquareEnix/FINAL FANTASY XIV";

pub struct WineRuntime {
    #[allow(dead_code)]
    pub root: PathBuf,
    pub prefix: PathBuf,
    pub wine_bin: PathBuf,
    #[allow(dead_code)]
    pub wineserver_bin: PathBuf,
    /// Additional `DYLD_FALLBACK_LIBRARY_PATH` entries (macOS only).
    pub dyld_fallback_paths: Vec<PathBuf>,
}

impl WineRuntime {
    #[allow(dead_code)] // used by the Linux backend; macOS derives paths differently.
    pub fn install_root(&self) -> PathBuf {
        self.prefix.join(PREFIX_FFXIV_SUBPATH)
    }

    pub fn configure_command(&self, cmd: &mut Command) {
        cmd.env("WINEPREFIX", &self.prefix);
        cmd.env("WINEDEBUG", "fixme-all,err-all");
        #[cfg(target_os = "macos")]
        if !self.dyld_fallback_paths.is_empty() {
            let joined = std::env::join_paths(&self.dyld_fallback_paths)
                .expect("wine runtime dyld paths contain no colons");
            cmd.env("DYLD_FALLBACK_LIBRARY_PATH", joined);
        }
    }

    /// Runs `wineserver -w` to let in-flight writes flush before we take
    /// action on the prefix contents.
    #[allow(dead_code)]
    pub fn wait_for_wineserver(&self) -> Result<()> {
        let mut cmd = Command::new(&self.wineserver_bin);
        cmd.arg("-w");
        self.configure_command(&mut cmd);
        let status = cmd.status().context("running wineserver -w")?;
        if !status.success() {
            return Err(anyhow!("wineserver -w exited with status {status:?}"));
        }
        Ok(())
    }
}

pub fn ensure_prefix_initialized(runtime: &WineRuntime) -> Result<()> {
    if runtime.prefix.join("system.reg").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&runtime.prefix)
        .with_context(|| format!("creating prefix {}", runtime.prefix.display()))?;
    let mut cmd = Command::new(&runtime.wine_bin);
    cmd.arg("wineboot").arg("--init");
    runtime.configure_command(&mut cmd);
    let status = cmd.status().context("running wineboot --init")?;
    if !status.success() {
        return Err(anyhow!("wineboot --init exited with status {status:?}"));
    }
    Ok(())
}

pub fn launch_ffxiv_game(
    runtime: &WineRuntime,
    exe_path: &Path,
    encoded_argument: &str,
) -> Result<()> {
    let mut cmd = Command::new(&runtime.wine_bin);
    cmd.arg(exe_path).arg(encoded_argument);
    if let Some(cwd) = exe_path.parent() {
        cmd.current_dir(cwd);
    }
    runtime.configure_command(&mut cmd);
    let status = cmd.status().context("launching ffxivgame.exe via wine")?;
    if !status.success() {
        return Err(anyhow!("ffxivgame.exe exited with status {status:?}"));
    }
    Ok(())
}

/// Copies `source_exe` to `dest_exe`, replacing `dest_exe` if it exists.
/// Intended for producing a fresh patched working copy per launch.
pub fn copy_exe_for_patching(source_exe: &Path, dest_exe: &Path) -> Result<()> {
    if let Some(parent) = dest_exe.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating dir {}", parent.display()))?;
    }
    std::fs::copy(source_exe, dest_exe).with_context(|| {
        format!(
            "copying {} -> {}",
            source_exe.display(),
            dest_exe.display()
        )
    })?;
    Ok(())
}
