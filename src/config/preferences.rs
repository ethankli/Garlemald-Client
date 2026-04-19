use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub launcher: LauncherPreferences,
    #[serde(default)]
    pub developer: DeveloperPreferences,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LauncherPreferences {
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub server_address: String,
    #[serde(default)]
    pub game_location: Option<PathBuf>,
    #[serde(default)]
    pub wine_runtime_dir: Option<PathBuf>,
    #[serde(default)]
    pub patch_download_dir: Option<PathBuf>,
}

/// Developer-only knobs surfaced in the Developer Settings dialog. Off by
/// default — enabling any of these trades runtime speed or log-file size
/// for visibility into the login / Now-Loading handshake.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeveloperPreferences {
    /// When true, overrides the default `WINEDEBUG` channel set with a
    /// verbose selection that covers DLL loads, winsock calls, structured
    /// exceptions, and thread ids. Wine's own output still lands in
    /// `<data_dir>/logs/wine.log`; this flag just raises the ceiling.
    #[serde(default)]
    pub enable_verbose_wine_debug: bool,
}

impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading preferences file {}", path.display()))?;
        let prefs: Self = toml::from_str(&text)
            .with_context(|| format!("parsing preferences file {}", path.display()))?;
        Ok(prefs)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config dir {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializing preferences")?;
        fs::write(path, text)
            .with_context(|| format!("writing preferences file {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preferences() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("prefs.toml");
        let mut prefs = Preferences::default();
        prefs.launcher.server_name = "Van Darnus Server".to_string();
        prefs.launcher.server_address = "vandarnus.seventhumbral.org".to_string();
        prefs.launcher.game_location = Some(PathBuf::from("/tmp/ffxiv"));
        prefs.save(&path).unwrap();

        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.launcher.server_name, prefs.launcher.server_name);
        assert_eq!(loaded.launcher.server_address, prefs.launcher.server_address);
        assert_eq!(loaded.launcher.game_location, prefs.launcher.game_location);
    }

    #[test]
    fn missing_file_yields_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("prefs.toml");
        let prefs = Preferences::load(&path).unwrap();
        assert!(prefs.launcher.server_name.is_empty());
    }
}
