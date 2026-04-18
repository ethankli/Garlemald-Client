use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use crc32fast::Hasher as Crc32Hasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadResult {
    Success,
    AlreadyUpToDate,
    BadChecksum,
    BadFileSize,
    Network,
    Cancelled,
}

/// Live progress snapshot for a single download.
pub struct DownloadProgress {
    pub bytes_downloaded: Arc<AtomicU64>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl DownloadProgress {
    pub fn new() -> Self {
        Self {
            bytes_downloaded: Arc::new(AtomicU64::new(0)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn bytes(&self) -> u64 {
        self.bytes_downloaded.load(Ordering::Relaxed)
    }
}

pub struct Downloader {
    progress: DownloadProgress,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        Self { progress: DownloadProgress::new() }
    }

    pub fn progress(&self) -> &DownloadProgress {
        &self.progress
    }

    /// Downloads `src_url` into `dst_path`, skipping the download if the
    /// target already exists with the expected size + CRC32.
    pub fn download(
        &self,
        src_url: &str,
        dst_path: &Path,
        expected_size: u64,
        expected_crc32: u32,
    ) -> Result<DownloadResult> {
        self.progress.bytes_downloaded.store(0, Ordering::SeqCst);
        self.progress.cancel_flag.store(false, Ordering::SeqCst);

        if download_already_valid(dst_path, expected_size, expected_crc32).unwrap_or(false) {
            return Ok(DownloadResult::AlreadyUpToDate);
        }

        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating dir {}", parent.display()))?;
        }

        let response = match ureq::get(src_url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_code, _r)) => return Ok(DownloadResult::Network),
            Err(ureq::Error::Transport(_)) => return Ok(DownloadResult::Network),
        };

        let tmp_path = tmp_download_path(dst_path);
        let mut output = File::create(&tmp_path)
            .with_context(|| format!("creating {}", tmp_path.display()))?;
        let mut hasher = Crc32Hasher::new();

        let mut reader = response.into_reader();
        let mut buf = [0u8; 0x10000];
        loop {
            if self.progress.cancel_flag.load(Ordering::Relaxed) {
                drop(output);
                let _ = fs::remove_file(&tmp_path);
                return Ok(DownloadResult::Cancelled);
            }
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => return Ok(DownloadResult::Network),
            };
            output.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            self.progress
                .bytes_downloaded
                .fetch_add(n as u64, Ordering::Relaxed);
        }

        let downloaded_size = self.progress.bytes();
        if downloaded_size != expected_size {
            let _ = fs::remove_file(&tmp_path);
            return Ok(DownloadResult::BadFileSize);
        }
        if hasher.finalize() != expected_crc32 {
            let _ = fs::remove_file(&tmp_path);
            return Ok(DownloadResult::BadChecksum);
        }

        fs::rename(&tmp_path, dst_path)
            .with_context(|| format!("renaming {} -> {}", tmp_path.display(), dst_path.display()))?;
        Ok(DownloadResult::Success)
    }
}

fn download_already_valid(path: &Path, expected_size: u64, expected_crc32: u32) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() != expected_size {
        return Ok(false);
    }
    let crc = compute_file_crc32(path)?;
    Ok(crc == expected_crc32)
}

pub fn compute_file_crc32(path: &Path) -> Result<u32> {
    let file = File::open(path)
        .with_context(|| format!("opening {} for CRC32", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Crc32Hasher::new();
    let mut buf = [0u8; 0x4000];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

fn tmp_download_path(dst: &Path) -> PathBuf {
    let mut s = dst.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Convenience: compute the full URL for a manifest entry relative to
/// [`super::manifest::PATCH_URL_BASE`].
#[allow(dead_code)]
pub fn patch_url(entry: &super::manifest::PatchEntry) -> String {
    format!("{}{}", super::manifest::PATCH_URL_BASE, entry.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_before_download_is_sticky() {
        let d = Downloader::new();
        d.progress().cancel();
        assert!(d.progress().cancel_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn compute_crc_matches_known_string() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("known.bin");
        fs::write(&path, b"123456789").unwrap();
        // CRC32 of "123456789" is 0xCBF43926 per the standard reference vector.
        assert_eq!(compute_file_crc32(&path).unwrap(), 0xCBF43926);
    }
}
