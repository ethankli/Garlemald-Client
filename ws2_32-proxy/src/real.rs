//! Late-binding to the real `C:\windows\system32\ws2_32.dll`.
//!
//! Our DLL sits next to `ffxivgame.patched.exe` so the loader picks us
//! up first whenever the exe does `LoadLibrary("ws2_32.dll")` or
//! resolves an import-table entry. To forward the eight functions we
//! wrap to the real implementation, we need to find the real DLL and
//! `GetProcAddress` each symbol.
//!
//! The lookup is done once at `DLL_PROCESS_ATTACH` (called from
//! [`crate::DllMain`]) via [`bind`]. Each hooked function reads its
//! pointer through the `Option<...>` getters below; a `None` means the
//! resolve failed and the hook short-circuits to a `-1` return, which
//! the client will surface as `WSAENOTSOCK` / similar. In practice
//! every Windows ws2_32 exports every function we ask for, so `None`
//! only happens when the loader refused to find the real DLL at all —
//! at which point the game process would already be broken regardless
//! of us.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::Networking::WinSock::{SOCKADDR, SOCKET};
use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

use crate::WSABUF;

// -------------------------------------------------------------------------
// Function-pointer types. Each mirrors the Windows SDK signature exactly
// so the cast from `FARPROC` is safe.
// -------------------------------------------------------------------------

pub type SendFn = unsafe extern "system" fn(SOCKET, *const i8, i32, i32) -> i32;
pub type RecvFn = unsafe extern "system" fn(SOCKET, *mut i8, i32, i32) -> i32;
pub type ConnectFn = unsafe extern "system" fn(SOCKET, *const SOCKADDR, i32) -> i32;
pub type CloseFn = unsafe extern "system" fn(SOCKET) -> i32;
pub type BindFn = unsafe extern "system" fn(SOCKET, *const SOCKADDR, i32) -> i32;
pub type ListenFn = unsafe extern "system" fn(SOCKET, i32) -> i32;
pub type WsaGetLastErrorFn = unsafe extern "system" fn() -> i32;
pub type WsaSendFn = unsafe extern "system" fn(
    SOCKET,
    *const WSABUF,
    u32,
    *mut u32,
    u32,
    *mut core::ffi::c_void,
    *mut core::ffi::c_void,
) -> i32;
pub type WsaRecvFn = unsafe extern "system" fn(
    SOCKET,
    *mut WSABUF,
    u32,
    *mut u32,
    *mut u32,
    *mut core::ffi::c_void,
    *mut core::ffi::c_void,
) -> i32;

// -------------------------------------------------------------------------
// Resolved pointers. `OnceLock` makes them unambiguously const after
// `bind`; hot paths read without synchronisation.
// -------------------------------------------------------------------------

struct Resolved {
    send: Option<SendFn>,
    recv: Option<RecvFn>,
    connect: Option<ConnectFn>,
    closesocket: Option<CloseFn>,
    wsasend: Option<WsaSendFn>,
    wsarecv: Option<WsaRecvFn>,
    sock_bind: Option<BindFn>,
    listen: Option<ListenFn>,
    wsa_get_last_error: Option<WsaGetLastErrorFn>,
}

static RESOLVED: OnceLock<Resolved> = OnceLock::new();

/// Load the real DLL and resolve the eight wrapped functions. Safe to
/// call more than once (the `OnceLock` swallows repeats). Called from
/// `DllMain(PROCESS_ATTACH)` but also re-tried lazily from any hook
/// in case a pathological load order skips the attach.
pub fn bind() {
    RESOLVED.get_or_init(|| {
        // Load via absolute path so the loader doesn't come back to
        // our proxy (which is cached under the exe-directory path).
        // `thunks::system_ws2_32_handle` walks syswow64 → system32.
        let handle = match crate::thunks::system_ws2_32_handle() {
            Some(h) => h,
            None => {
                return Resolved {
                    send: None,
                    recv: None,
                    connect: None,
                    closesocket: None,
                    wsasend: None,
                    wsarecv: None,
                    sock_bind: None,
                    listen: None,
                    wsa_get_last_error: None,
                };
            }
        };
        Resolved {
            send: resolve(handle, c"send"),
            recv: resolve(handle, c"recv"),
            connect: resolve(handle, c"connect"),
            closesocket: resolve(handle, c"closesocket"),
            wsasend: resolve(handle, c"WSASend"),
            wsarecv: resolve(handle, c"WSARecv"),
            sock_bind: resolve(handle, c"bind"),
            listen: resolve(handle, c"listen"),
            wsa_get_last_error: resolve(handle, c"WSAGetLastError"),
        }
    });
}

/// Resolve a named export against the real DLL. Returns `None` when
/// `GetProcAddress` hands back null — which, for the names we ask
/// for, shouldn't happen on any shipped Windows build.
fn resolve<F>(handle: HMODULE, name: &core::ffi::CStr) -> Option<F> {
    let raw = unsafe { GetProcAddress(handle, name.as_ptr() as *const u8) };
    raw.map(|ptr| unsafe { core::mem::transmute_copy::<_, F>(&ptr) })
}

// -------------------------------------------------------------------------
// Accessors. A missing pointer means `bind` hasn't run yet or failed —
// either way the caller should skip the forward and return an error
// code rather than blindly dereferencing.
// -------------------------------------------------------------------------

fn resolved() -> Option<&'static Resolved> {
    if RESOLVED.get().is_none() {
        bind();
    }
    RESOLVED.get()
}

pub fn send() -> Option<SendFn> {
    resolved().and_then(|r| r.send)
}
pub fn recv() -> Option<RecvFn> {
    resolved().and_then(|r| r.recv)
}
pub fn connect() -> Option<ConnectFn> {
    resolved().and_then(|r| r.connect)
}
pub fn closesocket() -> Option<CloseFn> {
    resolved().and_then(|r| r.closesocket)
}
pub fn wsasend() -> Option<WsaSendFn> {
    resolved().and_then(|r| r.wsasend)
}
pub fn wsarecv() -> Option<WsaRecvFn> {
    resolved().and_then(|r| r.wsarecv)
}
pub fn sock_bind() -> Option<BindFn> {
    resolved().and_then(|r| r.sock_bind)
}
pub fn listen() -> Option<ListenFn> {
    resolved().and_then(|r| r.listen)
}
pub fn wsa_get_last_error() -> Option<WsaGetLastErrorFn> {
    resolved().and_then(|r| r.wsa_get_last_error)
}
