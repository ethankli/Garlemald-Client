//! Background driver for the patcher phase. Runs on its own thread so the
//! egui UI stays responsive; the UI polls [`PatcherShared`] each frame.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use parking_lot::Mutex;

use super::downloader::{DownloadProgress, DownloadResult, Downloader};
use super::manifest::{total_bytes, PATCH_MANIFEST, PATCH_URL_BASE};
use super::process::{write_version_files, PatchPlan};

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
}

impl Phase {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Phase::Downloading,
            2 => Phase::Patching,
            3 => Phase::Done,
            4 => Phase::Error,
            5 => Phase::Cancelled,
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

/// Spawns the download+apply worker. The returned [`JoinHandle`] lets the UI
/// detach or wait on the worker; typical use is to let it run and poll the
/// shared state for updates.
pub fn start_patcher_worker(
    shared: Arc<PatcherShared>,
    game_dir: PathBuf,
    download_dir: PathBuf,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("garlemald-patcher".into())
        .spawn(move || run_patcher(shared, game_dir, download_dir))
        .expect("failed to spawn patcher thread")
}

fn run_patcher(shared: Arc<PatcherShared>, game_dir: PathBuf, download_dir: PathBuf) {
    shared.set_phase(Phase::Downloading);

    let downloader = Downloader::with_progress(shared.download.clone());

    for (idx, entry) in PATCH_MANIFEST.iter().enumerate() {
        if shared.is_cancel_requested() {
            shared.set_phase(Phase::Cancelled);
            return;
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
                return;
            }
            Ok(DownloadResult::BadChecksum) => {
                shared.set_error(format!("Download failed: bad checksum for {}", entry.path));
                return;
            }
            Ok(DownloadResult::BadFileSize) => {
                shared.set_error(format!("Download failed: bad file size for {}", entry.path));
                return;
            }
            Ok(DownloadResult::Network) => {
                shared.set_error(format!("Download failed: network error on {}", entry.path));
                return;
            }
            Err(e) => {
                shared.set_error(format!("Download error on {}: {e}", entry.path));
                return;
            }
        }
    }

    shared.set_phase(Phase::Patching);

    let plan = match PatchPlan::from_download_dir(&download_dir) {
        Ok(p) => p,
        Err(e) => {
            shared.set_error(format!("Failed to plan patches: {e}"));
            return;
        }
    };

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

