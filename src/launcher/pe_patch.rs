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

/// File offset of the indirect `call *[0x012651B4]` inside the game's
/// fatal-assert handler (entry at `0x00A48B30`). See `ASSERT_LOG_PATCH_BYTES`.
pub const ASSERT_LOG_PATCH_RVA: u32 = 0x648BBF;

/// Replaces a `call <unix-time-from-GetSystemTimeAsFileTime helper>`
/// (5-byte `E8 rel32`) with `mov eax, 0x50E0E812` (5-byte `B8 imm32`),
/// pinning the game's notion of "current server UTC" to
/// `0x50E0E812` = 1356916754 (2012-12-31 01:19:14 UTC — around the day the
/// 1.x servers were retired). Without this, the game reads a 2026 timestamp
/// from the host and rejects it as far in the future of `SERVER_UTC`.
pub const ENCRYPTION_TIME_PATCH_BYTES: [u8; 5] = [0xB8, 0x12, 0xE8, 0xE0, 0x50];

/// Replaces the 16-byte `call *[0x012651B4] ; movl $0, [0]` block at
/// `0x00A48BBF` inside the game's fatal-assert handler — the single call
/// that dispatches the formatted assertion text to a runtime-configurable
/// log callback, plus the deliberate null-pointer-write trap immediately
/// after.
///
/// Default behaviour: the callback at `[0x012651B4]` points at a no-op
/// `ret` stub (`0x006CE2E0`), so the assertion message is silently
/// dropped, then the trap fires and Wine reports `c0000005 at
/// 0x00A48BC5` with no clue which assertion failed.
///
/// New behaviour:
///
/// ```text
///   before:
///     A48BBF: FF 15 B4 51 26 01            call dword ptr [0x012651B4]
///     A48BC5: C7 05 00 00 00 00 00 00 00 00 movl $0, [0]   (trap)
///   after:
///     A48BBF: FF 15 64 E1 F3 00            call dword ptr [0x00F3E164]
///     A48BC5: 8D 64 24 FC                  lea esp, [esp-4]
///     A48BC9: 90 90 90 90 90 90            nop * 6
/// ```
///
/// The patch does two things at once:
///
/// 1. Swaps the indirect call for a direct `call *[0x00F3E164]` (IAT entry
///    for `OutputDebugStringA`). The assert handler's local buffer
///    pointer is in `edx`, pushed as the first argument; `OutputDebugStringA`
///    is stdcall and pops that 4-byte arg via `ret 4`. The original
///    handler was cdecl (no-op `ret`), so we have a 4-byte deficit.
/// 2. Replaces the trap with `lea esp, [esp-4]` to repay the deficit, so
///    the existing `addl $0x808, esp ; ret` epilogue at `0x00A48BCF`
///    restores the stack exactly and returns cleanly to the caller.
///
/// The leftover `6` (severity) and the dummy 4 bytes from the `lea` get
/// folded into the `0x808` epilogue restore — none of this is observable
/// to the caller. Callers of the assert handler are cdecl with 5
/// arguments and clean up via `add esp, 0x14` themselves.
///
/// Net effect: assertions that would have killed the process now log to
/// `OutputDebugStringA` and return. For the cinematic case the failing
/// assertion is `StretchRect` returning `WINEDDERR_SURFACEBUSY` from a
/// per-frame race in WineD3D's surface lock-tracking (the game allows
/// `StretchRect` from a locked surface; native D3D9 silently serialises,
/// WineD3D rejects). With the trap removed, the stale frame is dropped
/// and the next frame's StretchRect proceeds.
///
/// We deliberately do not touch the warning handler at `0x00A49380` —
/// that one already returns to its caller and is correctly cdecl, so it
/// needs no patch.
///
/// Pairs with `WINEDEBUG=...,+debugstr` in `WINEDEBUG_DEFAULT` so the
/// forwarded message lands in `wine.log`. The IAT entry for
/// `OutputDebugStringA` is at VMA `0x00F3E164`; this is stable across the
/// binary because relocations are stripped.
pub const ASSERT_LOG_PATCH_BYTES: [u8; 16] = [
    0xFF, 0x15, 0x64, 0xE1, 0xF3, 0x00, // call dword ptr [0x00F3E164]
    0x8D, 0x64, 0x24, 0xFC, // lea esp, [esp-4]   (repay stdcall's 4-byte pop)
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // nop * 6
];

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

pub fn assert_log_patch() -> PePatch {
    PePatch {
        rva: ASSERT_LOG_PATCH_RVA,
        bytes: ASSERT_LOG_PATCH_BYTES.to_vec(),
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

    #[test]
    fn assert_log_patch_is_sixteen_bytes() {
        let p = assert_log_patch();
        assert_eq!(p.bytes.len(), 16);
        assert_eq!(p.rva, ASSERT_LOG_PATCH_RVA);
        // call dword ptr [0x00F3E164]
        assert_eq!(&p.bytes[0..6], &[0xFF, 0x15, 0x64, 0xE1, 0xF3, 0x00]);
        // lea esp, [esp-4]
        assert_eq!(&p.bytes[6..10], &[0x8D, 0x64, 0x24, 0xFC]);
        // padding nops
        assert_eq!(&p.bytes[10..16], &[0x90; 6]);
    }
}
