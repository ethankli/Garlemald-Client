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

//! Port of `SeventhUmbral/launcher/PatchFile.cpp` — the FFXIV 1.x "ZIPATCH"
//! incremental-update file format.
//!
//! A patch starts with a 12-byte signature (`91 5A 49 50 41 54 43 48 0D 0A 1A 0A`,
//! PNG-inspired) followed by a stream of PNG-style chunks:
//!
//! | field  | bytes | notes                                             |
//! |--------|-------|---------------------------------------------------|
//! | size   | 4 BE  | length of `body`                                  |
//! | tag    | 4     | chunk type: FHDR / APLY / APFS / ADIR / DELD / ETRY |
//! | body   | size  | command-specific payload                          |
//! | crc    | 4 BE  | CRC32 of `tag` + `body` (we don't verify)         |
//!
//! Top-level chunks we care about:
//!
//! | tag    | semantics                                                        |
//! |--------|------------------------------------------------------------------|
//! | FHDR   | file header / version indicator (body opaque, we skip it)        |
//! | APLY   | opaque header metadata (two APLY chunks always follow FHDR)      |
//! | APFS   | opaque filesystem metadata (observed in some patches)            |
//! | ADIR   | "add directory": `pathlen:u32 BE, path, trailing metadata`       |
//! | DELD   | "delete directory": same layout as ADIR                          |
//! | ETRY   | per-file entry: path + one-or-more file-body records             |
//!
//! An ETRY body is:
//! `pathLen:u32 BE, path, itemCount:u32 BE, items`. Each item:
//! `hashMode:u32 LE` (0x41/0x44/0x4D) + 20-byte src hash + 20-byte dst hash +
//! `compressionMode:u32 LE` (0x4E=none, 0x5A=zlib) + `compressedSize:u32 BE` +
//! `previousSize:u32 BE` + `newSize:u32 BE`. Only the last item in the entry
//! carries a non-zero compressedSize with the actual body bytes.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use flate2::read::ZlibDecoder;

const MAGIC: [u8; 12] = [
    0x91, b'Z', b'I', b'P', b'A', b'T', b'C', b'H', 0x0D, 0x0A, 0x1A, 0x0A,
];

#[derive(Debug, Default)]
pub struct PatchApplyResult {
    pub messages: Vec<String>,
}

pub fn apply_patch_file(patch_path: &Path, game_root: &Path) -> Result<PatchApplyResult> {
    let file = File::open(patch_path)
        .with_context(|| format!("opening patch file {}", patch_path.display()))?;
    let mut reader = BufReader::new(file);

    let mut sig = [0u8; 12];
    reader
        .read_exact(&mut sig)
        .context("reading ZIPATCH signature")?;
    if sig != MAGIC {
        return Err(anyhow!(
            "{} is not a ZIPATCH patch file",
            patch_path.display()
        ));
    }

    let mut result = PatchApplyResult::default();

    loop {
        let size = match try_read_u32_be(&mut reader)? {
            Some(s) => s as u64,
            None => break,
        };
        let mut tag = [0u8; 4];
        reader.read_exact(&mut tag).context("reading chunk tag")?;

        let mut body = (&mut reader).take(size);
        match &tag {
            b"FHDR" | b"APLY" | b"APFS" => {
                // Opaque top-level metadata we don't use — drained below.
            }
            b"ADIR" => execute_adir(&mut body, game_root, &mut result)?,
            b"DELD" => execute_deld(&mut body, game_root, &mut result)?,
            b"ETRY" => execute_etry(&mut body, game_root, &mut result)?,
            other => {
                return Err(anyhow!(
                    "unhandled ZIPATCH chunk: {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        }
        // Drain any bytes the handler didn't consume so the next chunk's size
        // lands at the right offset.
        io::copy(&mut body, &mut io::sink()).context("draining chunk body")?;

        // Skip the chunk's trailing CRC32. We don't verify it.
        let mut crc = [0u8; 4];
        reader.read_exact(&mut crc).context("reading chunk CRC")?;
    }

    Ok(result)
}

fn execute_adir<R: Read>(
    reader: &mut R,
    game_root: &Path,
    result: &mut PatchApplyResult,
) -> Result<()> {
    let path = read_dir_path(reader)?;
    let full = join_patch_path(game_root, &path);
    if full.exists() {
        result.messages.push(format!(
            "Warning: Directory '{}' creation requested but directory already exists.",
            full.display()
        ));
    } else {
        fs::create_dir_all(&full)
            .with_context(|| format!("creating directory {}", full.display()))?;
    }
    Ok(())
}

fn execute_deld<R: Read>(
    reader: &mut R,
    game_root: &Path,
    result: &mut PatchApplyResult,
) -> Result<()> {
    let path = read_dir_path(reader)?;
    let full = join_patch_path(game_root, &path);
    if !full.exists() {
        result.messages.push(format!(
            "Warning: Directory '{}' deletion requested but directory doesn't exist.",
            full.display()
        ));
    } else {
        fs::remove_dir_all(&full)
            .with_context(|| format!("removing directory {}", full.display()))?;
    }
    Ok(())
}

fn execute_etry<R: Read>(
    reader: &mut R,
    game_root: &Path,
    result: &mut PatchApplyResult,
) -> Result<()> {
    let path_len = read_u32_be(reader)? as usize;
    let path = read_path(reader, path_len)?;
    let full_path = join_patch_path(game_root, &path);
    let parent = full_path
        .parent()
        .ok_or_else(|| anyhow!("ETRY path has no parent: {}", full_path.display()))?;

    if !parent.exists() {
        result.messages.push(format!(
            "Warning: Directory '{}' doesn't exist. Creating.",
            parent.display()
        ));
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }
    if !full_path.exists() {
        result.messages.push(format!(
            "Warning: File '{}' doesn't exist. Creating.",
            full_path.display()
        ));
    }

    let item_count = read_u32_be(reader)?;
    for i in 0..item_count {
        let hash_mode = read_u32_le(reader)?;
        if !(hash_mode == 0x41 || hash_mode == 0x44 || hash_mode == 0x4D) {
            return Err(anyhow!("unexpected ETRY hash mode: 0x{hash_mode:X}"));
        }
        let mut src_hash = [0u8; 20];
        let mut dst_hash = [0u8; 20];
        reader.read_exact(&mut src_hash)?;
        reader.read_exact(&mut dst_hash)?;

        let compression_mode = read_u32_le(reader)?;
        if !(compression_mode == 0x4E || compression_mode == 0x5A) {
            return Err(anyhow!(
                "unexpected compression mode: 0x{compression_mode:X}"
            ));
        }
        let compressed_size = read_u32_be(reader)?;
        let _previous_size = read_u32_be(reader)?;
        let _new_size = read_u32_be(reader)?;

        if i != item_count - 1 && compressed_size != 0 {
            return Err(anyhow!(
                "non-final ETRY item carries data (compressed_size={compressed_size})"
            ));
        }
        if compressed_size == 0 {
            continue;
        }

        let output = open_with_retry(&full_path)?;
        let mut writer = BufWriter::new(output);
        match compression_mode {
            0x4E => extract_uncompressed(reader, &mut writer, compressed_size)?,
            0x5A => extract_zlib(reader, &mut writer, compressed_size)?,
            _ => unreachable!(),
        }
        writer.flush()?;
    }

    Ok(())
}

fn read_dir_path<R: Read>(reader: &mut R) -> Result<String> {
    let path_len = read_u32_be(reader)? as usize;
    read_path(reader, path_len)
}

fn read_path<R: Read>(reader: &mut R, len: usize) -> Result<String> {
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn join_patch_path(game_root: &Path, rel: &str) -> PathBuf {
    // Patch paths use backslashes; normalize to native separator.
    let normalized: String = rel
        .chars()
        .map(|c| {
            if c == '\\' {
                std::path::MAIN_SEPARATOR
            } else {
                c
            }
        })
        .collect();
    game_root.join(normalized)
}

fn try_read_u32_be<R: Read>(reader: &mut R) -> Result<Option<u32>> {
    let mut buf = [0u8; 4];
    match reader.read_exact(&mut buf) {
        Ok(()) => Ok(Some(u32::from_be_bytes(buf))),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn read_u32_be<R: Read>(reader: &mut R) -> Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

fn read_u32_le<R: Read>(reader: &mut R) -> Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn open_with_retry(path: &Path) -> Result<File> {
    // Mirrors CreateOutputStdStreamWithRetry — on Windows, explorer.exe's icon
    // cache can transiently hold exe files open, so back off a couple of times.
    let mut last_err = None;
    for _ in 0..6 {
        match File::create(path) {
            Ok(f) => return Ok(f),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    Err(anyhow!(
        "failed to open {} for writing: {}",
        path.display(),
        last_err.expect("loop ran")
    ))
}

fn extract_uncompressed<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    size: u32,
) -> Result<()> {
    let mut remaining = size as usize;
    let mut buf = [0u8; 0x4000];
    while remaining > 0 {
        let to_read = remaining.min(buf.len());
        reader.read_exact(&mut buf[..to_read])?;
        writer.write_all(&buf[..to_read])?;
        remaining -= to_read;
    }
    Ok(())
}

fn extract_zlib<R: Read, W: Write>(reader: &mut R, writer: &mut W, size: u32) -> Result<()> {
    let limited = reader.take(size as u64);
    let mut decoder = ZlibDecoder::new(limited);
    io::copy(&mut decoder, writer).context("inflating ETRY body")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_zipatch_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("not-a-patch.bin");
        fs::write(&path, b"definitely not a patch").unwrap();
        let game = tmp.path().join("game");
        fs::create_dir_all(&game).unwrap();
        let err = apply_patch_file(&path, &game).unwrap_err();
        assert!(err.to_string().contains("not a ZIPATCH"));
    }

    #[test]
    fn join_patch_path_converts_backslashes() {
        let result = join_patch_path(Path::new("/game"), "subdir\\file.dat");
        assert!(result.to_string_lossy().contains("subdir"));
        assert!(result.to_string_lossy().contains("file.dat"));
    }
}
