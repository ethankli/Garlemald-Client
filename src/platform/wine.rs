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

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

use crate::config;

/// `WINEDEBUG` channel selection for launching the game.
///
/// Wine's debug syntax is `[class][+/-]channel`. Without a class prefix,
/// `-fixme` / `+err` treat those as *channel* names (which don't exist), so
/// the obvious-looking `-fixme,+err` is a silent no-op. Correct form:
///   * `fixme-all` — silence fixme for every channel
///   * `err+all`   — keep err class enabled (it's on by default, but explicit
///                    makes our intent clear)
///   * `+seh`      — all classes for the seh channel, so crashes and unhandled
///                    exceptions still surface
///
/// Callers that want more verbosity (e.g. `+relay,+module,+loaddll`) can set
/// `WINEDEBUG` in the environment; we only fill this in as a default.
const WINEDEBUG_DEFAULT: &str = "fixme-all,err+all,+seh";

/// Relative path inside the prefix to the FFXIV install root, matching the
/// default the InstallShield installer uses.
pub const PREFIX_FFXIV_SUBPATH: &str = "drive_c/Program Files (x86)/SquareEnix/FINAL FANTASY XIV";

/// Returns a 32-bit millisecond tick value compatible with what Wine's
/// `GetTickCount()` will report to the game process.
///
/// Wine implements `GetTickCount` on top of `clock_gettime(CLOCK_MONOTONIC)`
/// (ms since boot), NOT the wall-clock. The Blowfish key used in the
/// command-line encryption is derived from the top 16 bits of this value, so
/// the launcher's tick and the game's tick must come from the same clock for
/// the keys to agree.
pub fn monotonic_ms_since_boot() -> u32 {
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if rc != 0 {
        return 0;
    }
    let ms = (ts.tv_sec as u64).wrapping_mul(1_000)
        + (ts.tv_nsec as u64 / 1_000_000);
    ms as u32
}

pub struct WineRuntime {
    #[allow(dead_code)]
    pub root: PathBuf,
    pub prefix: PathBuf,
    pub wine_bin: PathBuf,
    #[allow(dead_code)]
    pub wineserver_bin: PathBuf,
    /// Additional `DYLD_FALLBACK_LIBRARY_PATH` entries (macOS only).
    pub dyld_fallback_paths: Vec<PathBuf>,
    /// `gstreamer-1.0` plugin directory shipped in the runtime bundle. When
    /// `Some`, gets exported as `GST_PLUGIN_PATH` / `GST_PLUGIN_SYSTEM_PATH`
    /// so Wine's `winegstreamer` finds these instead of any system / brew
    /// install at a different version.
    pub gst_plugin_path: Option<PathBuf>,
}

impl WineRuntime {
    #[allow(dead_code)] // used by the Linux backend; macOS derives paths differently.
    pub fn install_root(&self) -> PathBuf {
        self.prefix.join(PREFIX_FFXIV_SUBPATH)
    }

    pub fn configure_command(&self, cmd: &mut Command) {
        self.configure_command_with_debug(cmd, None);
    }

    /// Like [`configure_command`], but lets the caller inject a specific
    /// `WINEDEBUG` value (e.g. from the Developer Settings dialog). When
    /// `wine_debug` is `None`, behaviour matches the original default:
    /// honour the parent env, else fall back to [`WINEDEBUG_DEFAULT`].
    pub fn configure_command_with_debug(&self, cmd: &mut Command, wine_debug: Option<&str>) {
        cmd.env("WINEPREFIX", &self.prefix);
        if let Some(value) = wine_debug {
            cmd.env("WINEDEBUG", value);
        } else if std::env::var_os("WINEDEBUG").is_none() {
            cmd.env("WINEDEBUG", WINEDEBUG_DEFAULT);
        }
        #[cfg(target_os = "macos")]
        if !self.dyld_fallback_paths.is_empty() {
            let joined = std::env::join_paths(&self.dyld_fallback_paths)
                .expect("wine runtime dyld paths contain no colons");
            cmd.env("DYLD_FALLBACK_LIBRARY_PATH", joined);
        }
        if let Some(plugin_dir) = &self.gst_plugin_path {
            cmd.env("GST_PLUGIN_PATH", plugin_dir);
            cmd.env("GST_PLUGIN_SYSTEM_PATH", plugin_dir);
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
    wine_debug_override: Option<&str>,
) -> Result<()> {
    let log_path = wine_log_path()?;
    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .with_context(|| format!("opening wine log {}", log_path.display()))?;
    writeln!(
        &log_file,
        "=== garlemald-client launch ===\nwine: {}\nexe:  {}\narg:  {}\nprefix: {}\nWINEDEBUG: {}\n",
        runtime.wine_bin.display(),
        exe_path.display(),
        encoded_argument,
        runtime.prefix.display(),
        wine_debug_override.unwrap_or("(default)"),
    )
    .ok();

    let stderr_log = log_file
        .try_clone()
        .context("cloning wine log fd for stderr redirect")?;
    let stdout_log = log_file
        .try_clone()
        .context("cloning wine log fd for stdout redirect")?;

    let mut cmd = Command::new(&runtime.wine_bin);
    cmd.arg(exe_path)
        .arg(encoded_argument)
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    if let Some(cwd) = exe_path.parent() {
        cmd.current_dir(cwd);
    }
    runtime.configure_command_with_debug(&mut cmd, wine_debug_override);

    log::info!("launching ffxivgame via wine; output → {}", log_path.display());
    let status = cmd.status().context("launching ffxivgame.exe via wine")?;

    writeln!(&log_file, "\n=== exit: {status:?} ===").ok();

    if !status.success() {
        emit_log_tail(&log_path);
        return Err(anyhow!(
            "ffxivgame.exe exited with status {status:?}; see {} for wine output",
            log_path.display(),
        ));
    }
    Ok(())
}

/// Path where we write a fresh Wine stdout+stderr capture on every launch.
/// Lives next to the prefix under `<data_dir>/logs/wine.log`.
fn wine_log_path() -> Result<PathBuf> {
    let dir = config::data_dir()?.join("logs");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating wine log dir {}", dir.display()))?;
    Ok(dir.join("wine.log"))
}

/// Prints the tail of the wine log to our own logger so a failed launch
/// leaves immediately-visible breadcrumbs without forcing the user to open
/// the file themselves.
fn emit_log_tail(log_path: &Path) {
    const TAIL_BYTES: usize = 8 * 1024;
    match std::fs::read(log_path) {
        Ok(bytes) => {
            let start = bytes.len().saturating_sub(TAIL_BYTES);
            let tail = String::from_utf8_lossy(&bytes[start..]);
            log::error!("--- tail of {} ---", log_path.display());
            for line in tail.lines() {
                log::error!("wine: {line}");
            }
            log::error!("--- end of wine log tail ---");
        }
        Err(e) => log::error!("could not read wine log {}: {e}", log_path.display()),
    }
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
