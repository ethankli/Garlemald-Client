//! Logging facility.
//!
//! Writes a single line-oriented trace file next to `ffxivgame.patched.exe`
//! so the user can tail it without hunting through AppData. Every hooked
//! call produces a header line plus a `xxd`-style hex dump of its payload
//! (for `send`/`recv`/`WSASend`/`WSARecv`), or just the header for
//! control-plane calls (`connect`, `closesocket`).
//!
//! Writes are serialised through `Mutex<BufWriter<File>>`. The critical
//! section covers just the `write_all` + `flush`, so the hot path on a
//! busy socket is a short lock + I/O syscall — no allocation on the
//! lock-held path beyond the formatted string we've already built.
//!
//! The file is opened on first use (not in `DllMain`). Opening inside
//! `DllMain` is legal but the loader lock is held; our first socket call
//! happens long after `DllMain` returns so a deferred open avoids any
//! loader-lock interaction.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::Networking::WinSock::{SOCKADDR, SOCKET};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

use crate::WSABUF;

/// Resolve the path the current DLL was loaded from, then replace the
/// filename with `ws2_32-trace.log`. Uses `GetModuleHandleExW` with the
/// `FROM_ADDRESS` flag so we don't have to know our own HMODULE — the
/// address of this function is enough for the loader to map back to the
/// module it belongs to.
fn trace_log_path_next_to_self() -> Option<PathBuf> {
    let mut hmodule: HMODULE = core::ptr::null_mut();
    let ok = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            trace_log_path_next_to_self as *const core::ffi::c_void as *const u16,
            &mut hmodule,
        )
    };
    if ok == 0 || hmodule.is_null() {
        return None;
    }
    // `GetModuleFileNameW` writes the UTF-16 path of the module. MAX_PATH
    // is 260 under the default Win32 ABI; we oversize slightly to cover
    // long-path prefixes Wine might hand us.
    let mut buf = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(hmodule, buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 || len as usize >= buf.len() {
        return None;
    }
    let wide = &buf[..len as usize];
    let path_str = String::from_utf16_lossy(wide);
    let mut dll_path = PathBuf::from(path_str);
    // Swap `ws2_32.dll` → `ws2_32-trace.log` while keeping the directory.
    dll_path.set_file_name("ws2_32-trace.log");
    Some(dll_path)
}

// -------------------------------------------------------------------------
// Singleton file handle.
// -------------------------------------------------------------------------

struct Sink {
    file: Mutex<BufWriter<File>>,
}

static SINK: OnceLock<Option<Sink>> = OnceLock::new();

/// Open the trace file. Called from `DllMain(PROCESS_ATTACH)` — best-effort;
/// if the game dir isn't writable (unlikely on Wine-managed prefixes) we
/// fall back to silently disabling logging for the rest of the process.
pub fn open() {
    SINK.get_or_init(build);
}

fn build() -> Option<Sink> {
    // Resolve the log path relative to *our own DLL's directory*, not
    // the current working directory. `DllMain` runs inside the loader
    // before the game has set its CWD to the exe folder, so a relative
    // path like `"ws2_32-trace.log"` would land somewhere unpredictable
    // (sometimes `C:\windows\system32` under Wine, sometimes the dir
    // the user launched the shim from). Using the DLL's own folder
    // puts the trace next to `ffxivgame.patched.exe` — exactly where
    // `garlemald-client` looks for it.
    let path = trace_log_path_next_to_self().unwrap_or_else(|| {
        // If the HMODULE lookup somehow fails, fall back to the
        // current directory so we at least get *something*. This
        // shouldn't happen in practice — we only need our HMODULE,
        // which is always resolvable for a loaded DLL.
        PathBuf::from("ws2_32-trace.log")
    });
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    let mut buf = BufWriter::new(file);
    let _ = writeln!(
        buf,
        "# ws2_32-proxy trace opened at {}",
        timestamp_iso(),
    );
    let _ = buf.flush();
    Some(Sink {
        file: Mutex::new(buf),
    })
}

/// Flush and drop on `PROCESS_DETACH`. The OS would clean up anyway but
/// flushing avoids losing the tail of the trace on a clean exit.
pub fn close() {
    if let Some(Some(sink)) = SINK.get() {
        if let Ok(mut f) = sink.file.lock() {
            let _ = f.flush();
        }
    }
}

fn write_line(s: &str) {
    let Some(Some(sink)) = SINK.get() else { return };
    let Ok(mut f) = sink.file.lock() else { return };
    let _ = f.write_all(s.as_bytes());
    let _ = f.flush();
}

// -------------------------------------------------------------------------
// Formatting helpers.
// -------------------------------------------------------------------------

fn timestamp_iso() -> String {
    // RFC3339-ish with millisecond precision. We avoid `chrono` / `time`
    // to keep the cross-compile footprint minimal — manual arithmetic
    // against `SystemTime` is enough for a log timestamp.
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let ms = d.subsec_millis();
    // Very rough date math — we don't need leap-second-correct output,
    // we need something that sorts chronologically alongside the
    // server's `packet.log` timestamps.
    let days = secs / 86_400;
    let sod = secs % 86_400;
    let h = sod / 3600;
    let m = (sod % 3600) / 60;
    let s = sod % 60;
    // Unix-epoch day 0 was 1970-01-01, a Thursday. We emit `T<seconds>`
    // rather than yyyy-mm-dd to avoid porting a date-math library.
    format!("T{days}D{h:02}:{m:02}:{s:02}.{ms:03}")
}

fn hex_dump(out: &mut String, bytes: &[u8]) {
    for (row_idx, chunk) in bytes.chunks(16).enumerate() {
        let _ = write!(out, "  {:08x}:", row_idx * 16);
        for (i, b) in chunk.iter().enumerate() {
            if i == 8 {
                out.push(' ');
            }
            let _ = write!(out, " {b:02x}");
        }
        let pad = (16 - chunk.len()) * 3 + if chunk.len() <= 8 { 1 } else { 0 };
        for _ in 0..pad {
            out.push(' ');
        }
        out.push_str("  |");
        for b in chunk {
            let c = *b;
            if (0x20..0x7f).contains(&c) {
                out.push(c as char);
            } else {
                out.push('.');
            }
        }
        out.push_str("|\n");
    }
}

/// Header format: `<timestamp> tid=<tid> <fn> socket=<s> len=<len> rc=<rc>`.
fn header(fn_name: &str, s: SOCKET, len: i32, rc: i32) -> String {
    let tid = unsafe { GetCurrentThreadId() };
    format!(
        "{ts} tid={tid} {fn_name} socket={sock} len={len} rc={rc}\n",
        ts = timestamp_iso(),
        tid = tid,
        fn_name = fn_name,
        sock = s,
        len = len,
        rc = rc,
    )
}

// -------------------------------------------------------------------------
// Per-hook entry points. Kept narrow so the hot path stays tight.
// -------------------------------------------------------------------------

pub fn log_send(s: SOCKET, buf: *const i8, len: i32, _flags: i32, rc: i32) {
    let mut out = header("send", s, len, rc);
    if rc > 0 && !buf.is_null() {
        let n = (rc as usize).min(len.max(0) as usize);
        let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, n) };
        hex_dump(&mut out, slice);
    }
    write_line(&out);
}

pub fn log_recv(s: SOCKET, buf: *mut i8, len: i32, _flags: i32, rc: i32) {
    let mut out = header("recv", s, len, rc);
    if rc > 0 && !buf.is_null() {
        let n = (rc as usize).min(len.max(0) as usize);
        let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, n) };
        hex_dump(&mut out, slice);
    }
    write_line(&out);
}

pub fn log_connect(s: SOCKET, name: *const SOCKADDR, namelen: i32, rc: i32) {
    let mut out = header("connect", s, namelen, rc);
    if !name.is_null() && namelen > 0 {
        let slice = unsafe {
            core::slice::from_raw_parts(name as *const u8, namelen as usize)
        };
        hex_dump(&mut out, slice);
    }
    write_line(&out);
}

pub fn log_closesocket(s: SOCKET, rc: i32) {
    write_line(&header("closesocket", s, 0, rc));
}

/// `bind`/`listen` are logged to settle the open question of whether the
/// 1.23b client binds any fixed *local* ports. The client makes outbound
/// connections to the server, so a local bind would be unusual — and if
/// one clashed with another process it could surface as a client-side
/// failure, which is exactly the "port already bound" hypothesis raised
/// after the 2026-08 net test. The sockaddr is hex-dumped like `connect`
/// so the bound port/address is recoverable; on failure the caller passes
/// `WSAGetLastError()` so a clash (WSAEADDRINUSE = 10048) is visible.
pub fn log_bind(s: SOCKET, name: *const SOCKADDR, namelen: i32, rc: i32, wsa_err: i32) {
    let mut out = header("bind", s, namelen, rc);
    if rc != 0 {
        let _ = writeln!(out, "  WSAGetLastError={wsa_err}");
    }
    if !name.is_null() && namelen > 0 {
        let slice = unsafe { core::slice::from_raw_parts(name as *const u8, namelen as usize) };
        hex_dump(&mut out, slice);
    }
    write_line(&out);
}

pub fn log_listen(s: SOCKET, backlog: i32, rc: i32, wsa_err: i32) {
    let mut out = header("listen", s, backlog, rc);
    if rc != 0 {
        let _ = writeln!(out, "  WSAGetLastError={wsa_err}");
    }
    write_line(&out);
}

pub fn log_wsasend(
    s: SOCKET,
    bufs: *const WSABUF,
    count: u32,
    bytes_sent: *mut u32,
    rc: i32,
) {
    let total = unsafe { bytes_sent.as_ref().copied().unwrap_or(0) } as i32;
    let mut out = header("WSASend", s, total, rc);
    if !bufs.is_null() {
        for i in 0..count {
            let wb = unsafe { &*bufs.add(i as usize) };
            if wb.buf.is_null() {
                continue;
            }
            let _ = writeln!(out, "  buffer[{i}] len={}", wb.len);
            let slice =
                unsafe { core::slice::from_raw_parts(wb.buf as *const u8, wb.len as usize) };
            hex_dump(&mut out, slice);
        }
    }
    write_line(&out);
}

pub fn log_wsarecv(
    s: SOCKET,
    bufs: *mut WSABUF,
    count: u32,
    bytes_recvd: *mut u32,
    rc: i32,
) {
    let total = unsafe { bytes_recvd.as_ref().copied().unwrap_or(0) } as i32;
    let mut out = header("WSARecv", s, total, rc);
    if !bufs.is_null() && rc == 0 {
        // WSARecv fills the buffers from the front. `bytes_recvd` is
        // the total; each buffer was filled in order up to its capacity.
        let mut remaining = total as u32;
        for i in 0..count {
            if remaining == 0 {
                break;
            }
            let wb = unsafe { &*bufs.add(i as usize) };
            if wb.buf.is_null() {
                continue;
            }
            let n = remaining.min(wb.len);
            let _ = writeln!(out, "  buffer[{i}] len={n}");
            let slice =
                unsafe { core::slice::from_raw_parts(wb.buf as *const u8, n as usize) };
            hex_dump(&mut out, slice);
            remaining = remaining.saturating_sub(n);
        }
    }
    write_line(&out);
}
