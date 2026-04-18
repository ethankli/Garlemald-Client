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
