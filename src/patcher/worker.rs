// garlemald-client — cross-platform launcher for FINAL FANTASY XIV 1.x private servers
// Copyright (C) 2026  Samuel Stegall
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Background driver for the patcher phase. Runs on its own thread so the
//! egui UI stays responsive; the UI polls [`PatcherShared`] each frame.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crc32fast::Hasher as Crc32Hasher;
use parking_lot::Mutex;

use super::downloader::{DownloadProgress, DownloadResult, Downloader};
use super::manifest::{total_bytes, PatchEntry, PATCH_MANIFEST, PATCH_URL_BASE};
use super::process::{write_version_files, PatchPlan};

/// Where the worker should get the patch files from. `Download` fetches the
/// manifest from the S3 bucket; `Local` trusts a pre-existing directory
/// (typically another install's patch cache) and just validates + applies it.
pub enum PatchSource {
    Download { download_dir: PathBuf },
    Local { source_dir: PathBuf },
}

/// High-level phase reported by the worker. Serialized as `u8` into an
/// atomic so UI observers can read it without locking.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Starting = 0,
    Downloading = 1,
    Patching = 2,
    Done = 3,
    Error = 4,
    Cancelled = 5,
    Validating = 6,
}

impl Phase {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Phase::Downloading,
            2 => Phase::Patching,
            3 => Phase::Done,
            4 => Phase::Error,
            5 => Phase::Cancelled,
            6 => Phase::Validating,
            _ => Phase::Starting,
        }
    }
}

/// State shared between the patcher thread and the UI. Progress fields use
/// atomics so the UI can poll them lock-free each frame; the string fields
/// (error + warnings) are protected by a lightweight mutex.
pub struct PatcherShared {
    pub download: DownloadProgress,
    pub download_idx: AtomicUsize,
    pub previous_completed_bytes: AtomicU64,
    pub patch_idx: AtomicUsize,
    phase: AtomicU8,
    error_message: Mutex<Option<String>>,
    warnings: Mutex<Vec<String>>,
    pub total_download_bytes: u64,
    pub total_patches: usize,
}

impl PatcherShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            download: DownloadProgress::new(),
            download_idx: AtomicUsize::new(0),
            previous_completed_bytes: AtomicU64::new(0),
            patch_idx: AtomicUsize::new(0),
            phase: AtomicU8::new(Phase::Starting as u8),
            error_message: Mutex::new(None),
            warnings: Mutex::new(Vec::new()),
            total_download_bytes: total_bytes(),
            total_patches: PATCH_MANIFEST.len(),
        })
    }

    pub fn phase(&self) -> Phase {
        Phase::from_u8(self.phase.load(Ordering::Acquire))
    }

    pub fn error(&self) -> Option<String> {
        self.error_message.lock().clone()
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings.lock().clone()
    }

    pub fn request_cancel(&self) {
        self.download.cancel();
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.download.is_cancelled()
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.phase(),
            Phase::Done | Phase::Error | Phase::Cancelled
        )
    }

    fn set_phase(&self, phase: Phase) {
        self.phase.store(phase as u8, Ordering::Release);
    }

    fn set_error(&self, message: impl Into<String>) {
        *self.error_message.lock() = Some(message.into());
        self.set_phase(Phase::Error);
    }

    fn push_warning(&self, message: impl Into<String>) {
        self.warnings.lock().push(message.into());
    }
}

/// Spawns the download+apply (or validate+apply) worker. The returned
/// [`JoinHandle`] lets the UI detach or wait on the worker; typical use is to
/// let it run and poll the shared state for updates.
pub fn start_patcher_worker(
    shared: Arc<PatcherShared>,
    game_dir: PathBuf,
    source: PatchSource,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("garlemald-patcher".into())
        .spawn(move || run_patcher(shared, game_dir, source))
        .expect("failed to spawn patcher thread")
}

fn run_patcher(shared: Arc<PatcherShared>, game_dir: PathBuf, source: PatchSource) {
    let plan = match source {
        PatchSource::Download { download_dir } => match run_download_phase(&shared, &download_dir) {
            Some(plan) => plan,
            None => return,
        },
        PatchSource::Local { source_dir } => match run_validate_phase(&shared, &source_dir) {
            Some(plan) => plan,
            None => return,
        },
    };

    shared.set_phase(Phase::Patching);

    for (idx, patch_path) in plan.patches_in_order.iter().enumerate() {
        if shared.is_cancel_requested() {
            shared.set_phase(Phase::Cancelled);
            return;
        }
        shared.patch_idx.store(idx, Ordering::Release);

        match crate::patch_format::apply_patch_file(patch_path, &game_dir) {
            Ok(result) => {
                for msg in result.messages {
                    shared.push_warning(msg);
                }
            }
            Err(e) => {
                shared.set_error(format!(
                    "Applying patch {} failed: {e}",
                    patch_path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_else(|| patch_path.display().to_string())
                ));
                return;
            }
        }
    }

    if let Err(e) = write_version_files(&game_dir) {
        shared.push_warning(format!("Failed to write version files: {e}"));
    }

    shared.set_phase(Phase::Done);
}

fn run_download_phase(shared: &Arc<PatcherShared>, download_dir: &Path) -> Option<PatchPlan> {
    shared.set_phase(Phase::Downloading);

    let downloader = Downloader::with_progress(shared.download.clone());

    for (idx, entry) in PATCH_MANIFEST.iter().enumerate() {
        if shared.is_cancel_requested() {
            shared.set_phase(Phase::Cancelled);
            return None;
        }
        shared.download_idx.store(idx, Ordering::Release);
        let url = format!("{PATCH_URL_BASE}{}", entry.path);
        let dst = download_dir.join(entry.path);

        let result = downloader.download(&url, &dst, entry.size, entry.crc32);
        match result {
            Ok(DownloadResult::Success) | Ok(DownloadResult::AlreadyUpToDate) => {
                shared
                    .previous_completed_bytes
                    .fetch_add(entry.size, Ordering::Release);
            }
            Ok(DownloadResult::Cancelled) => {
                shared.set_phase(Phase::Cancelled);
                return None;
            }
            Ok(DownloadResult::BadChecksum) => {
                shared.set_error(format!("Download failed: bad checksum for {}", entry.path));
                return None;
            }
            Ok(DownloadResult::BadFileSize) => {
                shared.set_error(format!("Download failed: bad file size for {}", entry.path));
                return None;
            }
            Ok(DownloadResult::Network) => {
                shared.set_error(format!("Download failed: network error on {}", entry.path));
                return None;
            }
            Err(e) => {
                shared.set_error(format!("Download error on {}: {e}", entry.path));
                return None;
            }
        }
    }

    match PatchPlan::from_download_dir(download_dir) {
        Ok(p) => Some(p),
        Err(e) => {
            shared.set_error(format!("Failed to plan patches: {e}"));
            None
        }
    }
}

/// Local-install variant of the pre-apply phase: resolves each manifest entry
/// to a file in `source_dir`, verifies size + CRC32, and reports progress via
/// the same download progress atomics so the UI's existing bar can show it.
fn run_validate_phase(shared: &Arc<PatcherShared>, source_dir: &Path) -> Option<PatchPlan> {
    shared.set_phase(Phase::Validating);

    let plan = match PatchPlan::from_local_source(source_dir) {
        Ok(p) => p,
        Err(e) => {
            shared.set_error(format!("Local patch scan failed: {e}"));
            return None;
        }
    };

    // Pair each manifest entry to its resolved path (by leaf name). The plan
    // is already sorted by leaf; so is the manifest, but we don't rely on
    // that — look up each manifest entry's leaf in the plan.
    let pairs = pair_manifest_with_paths(&plan.patches_in_order);

    for (idx, (entry, path)) in pairs.iter().enumerate() {
        if shared.is_cancel_requested() {
            shared.set_phase(Phase::Cancelled);
            return None;
        }
        shared.download_idx.store(idx, Ordering::Release);
        // Reset the per-file counter so the UI shows "validated/size" for the
        // current file while `previous_completed_bytes` accumulates the rest.
        shared.download.bytes_downloaded.store(0, Ordering::SeqCst);

        match validate_file(path, entry.size, entry.crc32, shared) {
            Ok(true) => {
                shared
                    .previous_completed_bytes
                    .fetch_add(entry.size, Ordering::Release);
            }
            Ok(false) => {
                shared.set_phase(Phase::Cancelled);
                return None;
            }
            Err(ValidateError::SizeMismatch { actual }) => {
                shared.set_error(format!(
                    "Local patch {} has wrong size ({} bytes, expected {})",
                    path.display(),
                    actual,
                    entry.size,
                ));
                return None;
            }
            Err(ValidateError::Crc32Mismatch) => {
                shared.set_error(format!(
                    "Local patch {} failed CRC32 check",
                    path.display(),
                ));
                return None;
            }
            Err(ValidateError::Io(e)) => {
                shared.set_error(format!(
                    "Reading local patch {} failed: {e}",
                    path.display(),
                ));
                return None;
            }
        }
    }

    Some(plan)
}

fn pair_manifest_with_paths(paths: &[PathBuf]) -> Vec<(&'static PatchEntry, PathBuf)> {
    let mut out = Vec::with_capacity(PATCH_MANIFEST.len());
    for entry in PATCH_MANIFEST {
        let leaf = leaf_name(entry.path);
        if let Some(p) = paths
            .iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(leaf))
        {
            out.push((entry, p.clone()));
        }
    }
    out
}

fn leaf_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

enum ValidateError {
    SizeMismatch { actual: u64 },
    Crc32Mismatch,
    Io(std::io::Error),
}

impl From<std::io::Error> for ValidateError {
    fn from(e: std::io::Error) -> Self {
        ValidateError::Io(e)
    }
}

/// Returns `Ok(true)` when the file matches the expected size + CRC32, or
/// `Ok(false)` when the worker was cancelled mid-read. Streams the file so we
/// can update the shared progress counter as we go.
fn validate_file(
    path: &Path,
    expected_size: u64,
    expected_crc32: u32,
    shared: &Arc<PatcherShared>,
) -> Result<bool, ValidateError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if metadata.len() != expected_size {
        return Err(ValidateError::SizeMismatch {
            actual: metadata.len(),
        });
    }

    let mut reader = BufReader::new(file);
    let mut hasher = Crc32Hasher::new();
    let mut buf = [0u8; 0x10000];
    loop {
        if shared.is_cancel_requested() {
            return Ok(false);
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        shared
            .download
            .bytes_downloaded
            .fetch_add(n as u64, Ordering::Relaxed);
    }

    if hasher.finalize() != expected_crc32 {
        return Err(ValidateError::Crc32Mismatch);
    }
    Ok(true)
}
