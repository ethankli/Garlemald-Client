//! Port of `PatchProcess.cpp` helpers: version-file checks and post-patch
//! version-file writes. The actual driver loop (download N files, then apply
//! them in sorted order) lives in the UI layer since it needs to report
//! progress to the user.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::version::{FFXIV_BOOT_VERSION, FFXIV_GAME_VERSION};

pub struct PatchPlan {
    /// Absolute paths to the downloaded patch files, in application order
    /// (sorted by filename leaf so chronologically-later patches apply later).
    pub patches_in_order: Vec<PathBuf>,
}

impl PatchPlan {
    pub fn from_download_dir(download_dir: &Path) -> Result<Self> {
        let mut paths = Vec::new();
        for entry in crate::patcher::manifest::PATCH_MANIFEST {
            paths.push(download_dir.join(entry.path));
        }
        paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        Ok(Self { patches_in_order: paths })
    }
}

/// Returns `true` when the game's on-disk `game.ver` matches our expected version.
pub fn check_game_version(game_location: &Path) -> bool {
    let ver_path = game_location.join("game.ver");
    match fs::read_to_string(&ver_path) {
        Ok(text) => text.trim() == FFXIV_GAME_VERSION,
        Err(_) => false,
    }
}

/// Writes `boot.ver` and `game.ver` to `game_location` to mark it as updated.
pub fn write_version_files(game_location: &Path) -> Result<()> {
    let boot = game_location.join("boot.ver");
    let game = game_location.join("game.ver");
    fs::write(&boot, FFXIV_BOOT_VERSION)
        .with_context(|| format!("writing {}", boot.display()))?;
    fs::write(&game, FFXIV_GAME_VERSION)
        .with_context(|| format!("writing {}", game.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ver_file_is_out_of_date() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!check_game_version(tmp.path()));
    }

    #[test]
    fn matching_ver_file_is_up_to_date() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("game.ver"), FFXIV_GAME_VERSION).unwrap();
        assert!(check_game_version(tmp.path()));
    }

    #[test]
    fn write_version_files_creates_both() {
        let tmp = tempfile::tempdir().unwrap();
        write_version_files(tmp.path()).unwrap();
        assert!(tmp.path().join("boot.ver").exists());
        assert!(tmp.path().join("game.ver").exists());
    }

    #[test]
    fn plan_sorts_by_leaf_name() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = PatchPlan::from_download_dir(tmp.path()).unwrap();
        let leaves: Vec<_> = plan
            .patches_in_order
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        let mut sorted = leaves.clone();
        sorted.sort();
        assert_eq!(leaves, sorted);
    }
}
