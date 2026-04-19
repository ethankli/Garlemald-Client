//! Cross-platform launch entry point. Delegates to the active
//! [`crate::platform::Platform`] for the actual process creation + patching.

use std::path::PathBuf;

use anyhow::Result;

use super::pe_patch::PePatch;
use crate::platform::Platform;

#[derive(Debug, Clone)]
pub struct GameLaunchRequest {
    pub game_dir: PathBuf,
    pub lobby_host: String,
    pub session_id: String,
    /// Optional `WINEDEBUG` override. When `Some`, the Wine-based backends
    /// set this on the child process env instead of the built-in default.
    /// Ignored by the native Windows backend.
    pub wine_debug_override: Option<String>,
    /// When true, install the ws2_32 DLL-hijack proxy into the game
    /// folder before launch so send/recv calls are logged to
    /// `<game_dir>/ws2_32-trace.log`. When false, remove any previously
    /// installed proxy files so stock networking resumes. Wine-only —
    /// the native Windows backend ignores this.
    pub enable_winsock_proxy: bool,
}

#[derive(Debug, Clone)]
pub struct PatchSpec {
    pub patches: Vec<PePatch>,
}

pub fn launch_game(request: &GameLaunchRequest) -> Result<()> {
    crate::platform::current().launch_game(request)
}
