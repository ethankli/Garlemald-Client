use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("org", "seventhumbral", "garlemald-client")
        .context("could not resolve platform-specific project directories")
}

pub fn config_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().to_path_buf())
}

pub fn data_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.data_dir().to_path_buf())
}

pub fn preferences_file_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("preferences.toml"))
}

pub fn servers_file_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("servers.xml"))
}

/// Path to the repo-bundled starter config (`./configs/garlemald-client.toml`
/// next to the binary). Used as a fallback when the per-user preferences
/// file doesn't exist yet — mirrors the server's `configs/*.toml` layout so
/// a fresh clone picks up matching localhost defaults for lobby/world/map.
pub fn bundled_config_path() -> PathBuf {
    PathBuf::from("./configs/garlemald-client.toml")
}
