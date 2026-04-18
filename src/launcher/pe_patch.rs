//! Resolve RVA → file-offset in a Win32 PE image so we can patch
//! `ffxivgame.exe` on disk before running it under Wine. This replaces the
//! Windows-only WriteProcessMemory flow for non-Windows platforms.
//!
//! The two patches are the same as Launcher.cpp: a 5-byte server-UTC
//! immediate-load patch at RVA 0x9A15E3 (see `ENCRYPTION_TIME_PATCH_BYTES`),
//! and a NUL-terminated host-name string (max 0x14 bytes) written into the
//! slot at RVA 0xB90110.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use object::read::pe::PeFile32;
use object::{Object, ObjectSection};

pub const ENCRYPTION_TIME_PATCH_RVA: u32 = 0x9A15E3;
pub const LOBBY_HOST_NAME_RVA: u32 = 0xB90110;
pub const LOBBY_HOST_NAME_SLOT_SIZE: usize = 0x14;

/// Replaces a `call <unix-time-from-GetSystemTimeAsFileTime helper>`
/// (5-byte `E8 rel32`) with `mov eax, 0x50E0E812` (5-byte `B8 imm32`),
/// pinning the game's notion of "current server UTC" to
/// `0x50E0E812` = 1356916754 (2012-12-31 01:19:14 UTC — around the day the
/// 1.x servers were retired). Without this, the game reads a 2026 timestamp
/// from the host and rejects it as far in the future of `SERVER_UTC`.
pub const ENCRYPTION_TIME_PATCH_BYTES: [u8; 5] = [0xB8, 0x12, 0xE8, 0xE0, 0x50];

#[derive(Debug, Clone)]
pub struct PePatch {
    pub rva: u32,
    pub bytes: Vec<u8>,
}

pub fn encryption_time_patch() -> PePatch {
    PePatch {
        rva: ENCRYPTION_TIME_PATCH_RVA,
        bytes: ENCRYPTION_TIME_PATCH_BYTES.to_vec(),
    }
}

/// Builds the lobby-host patch. The slot is a fixed-size NUL-terminated
/// buffer of 0x14 bytes; longer hostnames are rejected.
pub fn lobby_host_patch(host: &str) -> Result<PePatch> {
    let bytes = host.as_bytes();
    if bytes.len() + 1 > LOBBY_HOST_NAME_SLOT_SIZE {
        return Err(anyhow!(
            "lobby host name too long: {} bytes (limit {} including NUL)",
            bytes.len(),
            LOBBY_HOST_NAME_SLOT_SIZE
        ));
    }
    let mut buf = Vec::with_capacity(bytes.len() + 1);
    buf.extend_from_slice(bytes);
    buf.push(0);
    Ok(PePatch {
        rva: LOBBY_HOST_NAME_RVA,
        bytes: buf,
    })
}

/// Writes each patch's bytes into `exe_path` at the file offset corresponding
/// to its RVA. This is a destructive edit of the file — callers should pass
/// a working copy of `ffxivgame.exe`, not the original.
pub fn apply_patches_on_disk(exe_path: &Path, patches: &[PePatch]) -> Result<()> {
    let data = std::fs::read(exe_path)
        .with_context(|| format!("reading {}", exe_path.display()))?;
    let pe = PeFile32::parse(&*data)
        .with_context(|| format!("parsing PE headers of {}", exe_path.display()))?;

    let mut plan: Vec<(u64, &[u8], Vec<u8>)> = Vec::with_capacity(patches.len());
    for patch in patches {
        let file_offset = rva_to_file_offset(&pe, patch.rva).ok_or_else(|| {
            anyhow!(
                "RVA 0x{:X} is not mapped to any PE section of {}",
                patch.rva,
                exe_path.display()
            )
        })?;
        let before_end = (file_offset as usize).saturating_add(patch.bytes.len());
        let before = if before_end <= data.len() {
            data[file_offset as usize..before_end].to_vec()
        } else {
            Vec::new()
        };
        log::info!(
            "PE patch: RVA 0x{:X} -> file offset 0x{:X} ({} bytes)\n    before: {}\n    after:  {}",
            patch.rva,
            file_offset,
            patch.bytes.len(),
            hex(&before),
            hex(&patch.bytes),
        );
        plan.push((file_offset, patch.bytes.as_slice(), before));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(exe_path)
        .with_context(|| format!("opening {} for writing", exe_path.display()))?;
    for (offset, bytes, _) in plan {
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
    }
    file.flush()?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02X}"));
    }
    s
}

fn rva_to_file_offset(pe: &PeFile32<'_>, rva: u32) -> Option<u64> {
    // `ObjectSection::address()` returns image_base + RVA for PE, not the RVA
    // itself — normalize by subtracting the image base so we compare RVAs.
    let image_base = pe.relative_address_base() as u32;
    for section in pe.sections() {
        let section_rva = (section.address() as u32).checked_sub(image_base)?;
        let vsize = section.size() as u32;
        if rva >= section_rva && rva < section_rva.saturating_add(vsize) {
            let (file_offset, _) = section.file_range()?;
            return Some(file_offset + (rva - section_rva) as u64);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_patch_appends_nul() {
        let p = lobby_host_patch("test.example").unwrap();
        assert_eq!(p.rva, LOBBY_HOST_NAME_RVA);
        assert_eq!(*p.bytes.last().unwrap(), 0);
        assert_eq!(&p.bytes[..p.bytes.len() - 1], b"test.example");
    }

    #[test]
    fn host_patch_rejects_oversize_input() {
        let too_long = "x".repeat(LOBBY_HOST_NAME_SLOT_SIZE);
        assert!(lobby_host_patch(&too_long).is_err());
    }

    #[test]
    fn encryption_patch_is_five_bytes() {
        let p = encryption_time_patch();
        assert_eq!(p.bytes.len(), 5);
        assert_eq!(p.rva, ENCRYPTION_TIME_PATCH_RVA);
    }
}
