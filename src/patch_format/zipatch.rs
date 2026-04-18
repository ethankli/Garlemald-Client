//! Port of `SeventhUmbral/launcher/PatchFile.cpp` — the FFXIV 1.x "ZIPATCH"
//! incremental-update file format.
//!
//! A patch is a 16-byte header (`91 5A 49 50 41 54 43 48` + 8 magic bytes)
//! followed by a stream of 4-byte command tags, each with a chunk of
//! command-specific payload:
//!
//! | tag    | semantics                                                        |
//! |--------|------------------------------------------------------------------|
//! | FHDR   | file header / version indicator (we assert 0x0200)               |
//! | DIFF   | 5×u32 of metadata we ignore                                      |
//! | HIST   | 5×u32 of metadata we ignore                                      |
//! | APLY   | 5×u32 of metadata we ignore                                      |
//! | ADIR   | "add directory": big-endian u32 pathlen, path, 16 bytes metadata |
//! | DELD   | "delete directory": same layout as ADIR                          |
//! | ETRY   | per-file entry: path + one-or-more file-body records             |
//!
//! An ETRY file-body record is:
//! `hashMode:u32 LE` (0x41/0x44/0x4D) + 20-byte src hash + 20-byte dst hash +
//! `compressionMode:u32 LE` (0x4E=none, 0x5A=zlib) + `compressedSize:u32 BE` +
//! `previousSize:u32 BE` + `newSize:u32 BE`. Only the last record in the entry
//! carries a non-zero compressedSize with the actual body bytes; the ETRY
//! payload ends with a trailing 8 bytes we skip.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use flate2::read::ZlibDecoder;

const MAGIC: [u8; 8] = [0x91, b'Z', b'I', b'P', b'A', b'T', b'C', b'H'];

#[derive(Debug, Default)]
pub struct PatchApplyResult {
    pub messages: Vec<String>,
}

pub fn apply_patch_file(patch_path: &Path, game_root: &Path) -> Result<PatchApplyResult> {
    let file = File::open(patch_path)
        .with_context(|| format!("opening patch file {}", patch_path.display()))?;
    let mut reader = BufReader::new(file);

    let mut header = [0u8; 16];
    reader.read_exact(&mut header).context("reading ZIPATCH header")?;
    if header[..8] != MAGIC {
        return Err(anyhow!("{} is not a ZIPATCH patch file", patch_path.display()));
    }

    let mut result = PatchApplyResult::default();

    loop {
        let mut tag = [0u8; 4];
        match reader.read_exact(&mut tag) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        match &tag {
            b"FHDR" => execute_fhdr(&mut reader)?,
            b"DIFF" | b"HIST" | b"APLY" => skip_bytes(&mut reader, 5 * 4)?,
            b"ADIR" => execute_adir(&mut reader, game_root, &mut result)?,
            b"DELD" => execute_deld(&mut reader, game_root, &mut result)?,
            b"ETRY" => execute_etry(&mut reader, game_root, &mut result)?,
            other => {
                return Err(anyhow!(
                    "unhandled ZIPATCH command: {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        }
    }

    Ok(result)
}

fn execute_fhdr<R: Read>(reader: &mut R) -> Result<()> {
    let version = read_u32_be(reader)?;
    if version != 0x0200 {
        return Err(anyhow!("unexpected FHDR version: 0x{version:X}"));
    }
    Ok(())
}

fn execute_adir<R: Read>(reader: &mut R, game_root: &Path, result: &mut PatchApplyResult) -> Result<()> {
    let (path, _meta) = read_dir_command(reader)?;
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

fn execute_deld<R: Read>(reader: &mut R, game_root: &Path, result: &mut PatchApplyResult) -> Result<()> {
    let (path, _meta) = read_dir_command(reader)?;
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

fn execute_etry<R: Read + Seek>(
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
        result
            .messages
            .push(format!("Warning: File '{}' doesn't exist. Creating.", full_path.display()));
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
            return Err(anyhow!("unexpected compression mode: 0x{compression_mode:X}"));
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

    // Trailing 8 bytes of per-entry metadata we don't use.
    reader.seek(SeekFrom::Current(8))?;
    Ok(())
}

fn read_dir_command<R: Read>(reader: &mut R) -> Result<(String, [u8; 16])> {
    let path_len = read_u32_be(reader)? as usize;
    let path = read_path(reader, path_len)?;
    let mut meta = [0u8; 16];
    reader.read_exact(&mut meta)?;
    Ok((path, meta))
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
        .map(|c| if c == '\\' { std::path::MAIN_SEPARATOR } else { c })
        .collect();
    game_root.join(normalized)
}

fn skip_bytes<R: Read>(reader: &mut R, n: usize) -> Result<()> {
    let mut buf = vec![0u8; n];
    reader.read_exact(&mut buf)?;
    Ok(())
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
