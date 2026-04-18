pub mod app;
pub mod config;
pub mod crypto;
pub mod launcher;
pub mod login;
pub mod patch_format;
pub mod patcher;
pub mod platform;
pub mod servers;
pub mod version;

use anyhow::Result;

pub fn run() -> Result<()> {
    app::run()
}
