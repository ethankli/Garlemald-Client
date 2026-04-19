//! ws2_32.dll hijack proxy.
//!
//! Windows searches the exe's own directory before `System32` when
//! resolving `LoadLibrary` / import-table lookups. Sitting a DLL named
//! `ws2_32.dll` next to `ffxivgame.patched.exe` therefore wins the race
//! against the real one. This file implements that winning DLL:
//!
//! - [`DllMain`] on `PROCESS_ATTACH` calls [`real::bind`] to load the real
//!   `C:\windows\system32\ws2_32.dll` by absolute path (so the loader
//!   doesn't recurse into *us*) and resolve function pointers for every
//!   export we wrap.
//! - The wrapped exports (`send`, `recv`, `WSASend`, `WSARecv`, `connect`,
//!   `closesocket`) log the call parameters to `<game_dir>/ws2_32-trace.log`
//!   and then delegate to the real function.
//! - Every *other* ws2_32 export is forwarded to the real DLL via PE
//!   `FORWARD` export entries in `ws2_32.def`. We never see those calls in
//!   our process, so they can't pay the logging overhead.
//!
//! The hooks run on the game's thread for every socket I/O. The module
//! must not panic (the crate compiles with `panic = "abort"`) and must
//! not block — writes go through a `Mutex<BufWriter<File>>` with short
//! critical sections only.

#![cfg(windows)]
#![allow(non_snake_case)]

use windows_sys::Win32::Foundation::{BOOL, HMODULE, TRUE};
use windows_sys::Win32::Networking::WinSock::{SOCKADDR, SOCKET};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

mod log;
mod real;
mod thunks;

// -------------------------------------------------------------------------
// Unwind runtime stub.
// -------------------------------------------------------------------------
//
// `cargo build --target i686-pc-windows-gnu` pulls in the rustup-shipped
// precompiled libstd, which was built with `panic = "unwind"`. A handful
// of rarely-taken paths inside libstd (the `fmt::format` panic hook,
// `Wtf8Buf::from_wide`, `String::from_utf8_lossy`) therefore carry
// unwinding landing pads that reference `_Unwind_Resume` from libgcc's
// exception runtime. Our crate sets `panic = "abort"` — so these pads
// are dead code for us — but the linker still wants the symbol present
// to resolve the static references.
//
// Providing a stub that just aborts satisfies the linker without
// allocating code on any real path: the stub is only reachable via an
// unwind we can't trigger (the `panic = "abort"` setting short-circuits
// every panic to `abort_internal` before it would ever unwind into this
// landing pad). If it *does* somehow get called, `intrinsics::abort()`
// cleanly kills the process rather than leaving the game in an
// inconsistent state.
#[cfg(all(windows, target_env = "gnu"))]
#[unsafe(no_mangle)]
pub extern "C" fn _Unwind_Resume() -> ! {
    std::process::abort();
}

// -------------------------------------------------------------------------
// DllMain — loader entry point. Run exactly once for PROCESS_ATTACH/DETACH.
// -------------------------------------------------------------------------

/// SAFETY: DllMain's signature is fixed by the Windows loader; the
/// `_reserved` parameter convention matters but we don't touch it.
/// Returning `FALSE` from PROCESS_ATTACH would tell the loader to bail
/// the whole process, so we return TRUE even when binding fails — a
/// failure just means every hook becomes a no-op forward via the real
/// DLL's FORWARD entries, and the game keeps running without traces.
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(
    _hinst: HMODULE,
    fdw_reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> BOOL {
    match fdw_reason {
        DLL_PROCESS_ATTACH => {
            // Bind both the six hook pointers and the ~120 JMP-thunk
            // pointers. `thunks::bind_all` loads the real ws2_32 via
            // an absolute syswow64 path and resolves every symbol we
            // forward; `real::bind` piggy-backs on the same handle
            // resolution for the hook targets.
            thunks::bind_all();
            real::bind();
            log::open();
        }
        DLL_PROCESS_DETACH => {
            log::close();
        }
        _ => {}
    }
    TRUE
}

// -------------------------------------------------------------------------
// Hooked exports.
// -------------------------------------------------------------------------
//
// Signatures come from `windows-sys::Win32::Networking::WinSock`. The
// calling convention on 32-bit Windows is stdcall; `extern "system"`
// selects stdcall on i686-pc-windows-gnu and cdecl on x86_64 (which
// is fine — we ship only the 32-bit build to match the 1.23b client).

/// `int send(SOCKET s, const char *buf, int len, int flags);`
#[unsafe(no_mangle)]
pub unsafe extern "system" fn send(
    s: SOCKET,
    buf: *const i8,
    len: i32,
    flags: i32,
) -> i32 {
    let rc = match real::send() {
        Some(f) => unsafe { f(s, buf, len, flags) },
        None => -1,
    };
    log::log_send(s, buf, len, flags, rc);
    rc
}

/// `int recv(SOCKET s, char *buf, int len, int flags);`
#[unsafe(no_mangle)]
pub unsafe extern "system" fn recv(
    s: SOCKET,
    buf: *mut i8,
    len: i32,
    flags: i32,
) -> i32 {
    let rc = match real::recv() {
        Some(f) => unsafe { f(s, buf, len, flags) },
        None => -1,
    };
    log::log_recv(s, buf, len, flags, rc);
    rc
}

/// `int connect(SOCKET s, const sockaddr *name, int namelen);`
#[unsafe(no_mangle)]
pub unsafe extern "system" fn connect(
    s: SOCKET,
    name: *const SOCKADDR,
    namelen: i32,
) -> i32 {
    let rc = match real::connect() {
        Some(f) => unsafe { f(s, name, namelen) },
        None => -1,
    };
    log::log_connect(s, name, namelen, rc);
    rc
}

/// `int closesocket(SOCKET s);`
#[unsafe(no_mangle)]
pub unsafe extern "system" fn closesocket(s: SOCKET) -> i32 {
    let rc = match real::closesocket() {
        Some(f) => unsafe { f(s) },
        None => -1,
    };
    log::log_closesocket(s, rc);
    rc
}

/// `int WSASend(SOCKET s, LPWSABUF lpBuffers, DWORD dwBufferCount,
///              LPDWORD lpNumberOfBytesSent, DWORD dwFlags,
///              LPWSAOVERLAPPED lpOverlapped,
///              LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);`
#[unsafe(no_mangle)]
pub unsafe extern "system" fn WSASend(
    s: SOCKET,
    lp_buffers: *const WSABUF,
    buffer_count: u32,
    bytes_sent: *mut u32,
    flags: u32,
    overlapped: *mut core::ffi::c_void,
    completion: *mut core::ffi::c_void,
) -> i32 {
    let rc = match real::wsasend() {
        Some(f) => unsafe {
            f(s, lp_buffers, buffer_count, bytes_sent, flags, overlapped, completion)
        },
        None => -1,
    };
    log::log_wsasend(s, lp_buffers, buffer_count, bytes_sent, rc);
    rc
}

/// `int WSARecv(SOCKET s, LPWSABUF lpBuffers, DWORD dwBufferCount,
///              LPDWORD lpNumberOfBytesRecvd, LPDWORD lpFlags,
///              LPWSAOVERLAPPED lpOverlapped,
///              LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);`
#[unsafe(no_mangle)]
pub unsafe extern "system" fn WSARecv(
    s: SOCKET,
    lp_buffers: *mut WSABUF,
    buffer_count: u32,
    bytes_recvd: *mut u32,
    flags: *mut u32,
    overlapped: *mut core::ffi::c_void,
    completion: *mut core::ffi::c_void,
) -> i32 {
    let rc = match real::wsarecv() {
        Some(f) => unsafe {
            f(s, lp_buffers, buffer_count, bytes_recvd, flags, overlapped, completion)
        },
        None => -1,
    };
    log::log_wsarecv(s, lp_buffers, buffer_count, bytes_recvd, rc);
    rc
}

// -------------------------------------------------------------------------
// WSABUF — matches the Windows SDK shape. `windows-sys` exposes it under
// `Win32::Networking::WinSock` but with `#[repr(C)]` fields we can't
// dereference in our log path without pulling the feature flag through;
// redefining locally keeps the dependency surface narrow.
// -------------------------------------------------------------------------

#[repr(C)]
pub struct WSABUF {
    pub len: u32,
    pub buf: *mut i8,
}
