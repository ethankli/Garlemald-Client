//! One naked-asm JMP thunk per un-hooked `ws2_32.dll` export.
//!
//! PE-level `FORWARD` entries in a `.def` file resolve to whatever DLL
//! the loader finds under the given basename. Since our proxy is *also*
//! named `ws2_32.dll`, forwarding to `ws2_32.accept` would land back on
//! us (infinite recursion). The previous workaround — copying Wine's
//! builtin `ws2_32.dll` into the game folder as `ws2_32_real.dll` —
//! tripped over Wine's Unix-side thunking, whose `DllMain` expects the
//! PE basename to match one of Wine's registered builtins and fails
//! `c0000142` (`STATUS_DLL_INIT_FAILED`) otherwise.
//!
//! Instead, every un-hooked export is a Rust function of the form
//!
//! ```text
//! #[unsafe(naked)]
//! pub extern "system" fn accept() {
//!     naked_asm!("jmp dword ptr [{0}]", sym REAL_ACCEPT);
//! }
//! ```
//!
//! which assembles to a single 32-bit indirect JMP. The target
//! (`REAL_ACCEPT`) is populated at `DllMain(PROCESS_ATTACH)` by
//! [`bind_all`] — it calls `LoadLibraryA("C:\\windows\\syswow64\\ws2_32.dll")`
//! with an absolute path (so the loader skips the search order and
//! doesn't come back to us) and `GetProcAddress`es every symbol we
//! need. Caller's pushed args and return address sit untouched on the
//! stack; the real function's `ret N` returns directly to the caller,
//! so our thunks are transparent to stdcall semantics at any arity.
//!
//! The six names we *do* intercept (`send`, `recv`, `WSASend`,
//! `WSARecv`, `connect`, `closesocket`) live in `lib.rs` — not here —
//! because they need logging side-effects around the forward.

#![allow(non_upper_case_globals)]

use core::arch::naked_asm;
use core::ffi::{c_void, CStr};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

// -------------------------------------------------------------------------
// Per-export static function-pointer slots.
//
// `static mut` is permitted in naked-function `sym` references and the
// assembler emits a 32-bit relocation against each. Accesses from Rust
// code are behind `unsafe`. We never race on these: writes happen once
// at `DllMain` (inside the loader lock), reads happen afterward from
// game threads.
// -------------------------------------------------------------------------

macro_rules! decl_thunk {
    // Forward-only: `accept` with a storage slot `REAL_accept`.
    ($name:ident) => {
        paste_concat_ident!(static_ptr REAL_ $name);
        paste_concat_ident!(thunk_fn $name REAL_ $name);
    };
}

// `paste_concat_ident!` isn't a real macro; `paste` crate would do it.
// We avoid the dependency: fall back to spelling out each static and
// each thunk pair explicitly below. The macro above is kept in the
// file as a shape reference for anyone extending this later.

// ---- Storage slots ------------------------------------------------------
//
// One `static mut usize` per forwarded export. `usize` so the bit-width
// is automatic: 32-bit on our target, pointer-sized everywhere else.

macro_rules! real_slots {
    ($($name:ident),* $(,)?) => {
        $(
            // Crate-private storage — no `#[no_mangle]` so the linker
            // doesn't export these as DLL symbols. The naked thunks
            // still reference them by Rust name via `sym`, which the
            // assembler resolves to a 32-bit relocation against the
            // per-static symbol after Rust name-mangling.
            pub(crate) static mut $name: usize = 0;
        )*
    };
}

#[rustfmt::skip]
real_slots! {
    // NB: `bind` and `listen` are logging hooks in `lib.rs` (they resolve
    // their real pointers through `real.rs`, not these slots), so they are
    // deliberately not listed here.
    REAL_accept, REAL_getpeername, REAL_getsockname,
    REAL_getsockopt, REAL_htonl, REAL_htons, REAL_ioctlsocket,
    REAL_inet_addr, REAL_inet_ntoa, REAL_ntohl, REAL_ntohs,
    REAL_recvfrom, REAL_select, REAL_sendto, REAL_setsockopt,
    REAL_shutdown, REAL_socket,
    REAL_FreeAddrInfoEx, REAL_FreeAddrInfoExW, REAL_FreeAddrInfoW,
    REAL_GetAddrInfoExCancel, REAL_GetAddrInfoExOverlappedResult,
    REAL_GetAddrInfoExW, REAL_GetAddrInfoW, REAL_GetHostNameW,
    REAL_GetNameInfoW, REAL_InetNtopW, REAL_InetPtonW,
    REAL_WSApSetPostRoutine, REAL_WPUCompleteOverlappedRequest,
    REAL_WSAAccept, REAL_WSAAddressToStringA, REAL_WSAAddressToStringW,
    REAL_WSACloseEvent, REAL_WSAConnect, REAL_WSAConnectByNameA,
    REAL_WSAConnectByNameW, REAL_WSACreateEvent, REAL_WSADuplicateSocketA,
    REAL_WSADuplicateSocketW, REAL_WSAEnumNameSpaceProvidersA,
    REAL_WSAEnumNameSpaceProvidersW, REAL_WSAEnumNetworkEvents,
    REAL_WSAEnumProtocolsA,
    REAL_gethostbyaddr, REAL_gethostbyname, REAL_getprotobyname,
    REAL_getprotobynumber, REAL_getservbyname, REAL_getservbyport,
    REAL_gethostname,
    REAL_WSAEnumProtocolsW, REAL_WSAEventSelect, REAL_WSAGetOverlappedResult,
    REAL_WSAGetQOSByName, REAL_WSAGetServiceClassInfoA,
    REAL_WSAGetServiceClassInfoW,
    REAL_WSAGetServiceClassNameByClassIdA,
    REAL_WSAGetServiceClassNameByClassIdW, REAL_WSAHtonl, REAL_WSAHtons,
    REAL_WSAInstallServiceClassA, REAL_WSAInstallServiceClassW,
    REAL_WSAIoctl, REAL_WSAJoinLeaf, REAL_WSALookupServiceBeginA,
    REAL_WSALookupServiceBeginW, REAL_WSALookupServiceEnd,
    REAL_WSALookupServiceNextA, REAL_WSALookupServiceNextW,
    REAL_WSANSPIoctl, REAL_WSANtohl, REAL_WSANtohs, REAL_WSAPoll,
    REAL_WSAProviderConfigChange, REAL_WSARecvDisconnect, REAL_WSARecvFrom,
    REAL_WSARemoveServiceClass, REAL_WSAResetEvent,
    REAL_WSASendDisconnect, REAL_WSASendMsg, REAL_WSASendTo,
    REAL_WSASetEvent, REAL_WSASetServiceA, REAL_WSASetServiceW,
    REAL_WSASocketA, REAL_WSASocketW, REAL_WSAStringToAddressA,
    REAL_WSAStringToAddressW, REAL_WSAWaitForMultipleEvents,
    REAL_WSCDeinstallProvider, REAL_WSCEnableNSProvider,
    REAL_WSAAsyncSelect, REAL_WSAAsyncGetHostByAddr,
    REAL_WSAAsyncGetHostByName, REAL_WSAAsyncGetProtoByNumber,
    REAL_WSAAsyncGetProtoByName, REAL_WSAAsyncGetServByPort,
    REAL_WSAAsyncGetServByName, REAL_WSACancelAsyncRequest,
    REAL_WSASetBlockingHook, REAL_WSAUnhookBlockingHook,
    REAL_WSAGetLastError, REAL_WSASetLastError,
    REAL_WSACancelBlockingCall, REAL_WSAIsBlocking, REAL_WSAStartup,
    REAL_WSACleanup, REAL_WSCEnumProtocols, REAL_WSCGetProviderInfo,
    REAL_WSCGetProviderPath, REAL_WSCInstallNameSpace,
    REAL_WSCInstallProvider, REAL_WSCSetApplicationCategory,
    REAL_WSCUnInstallNameSpace, REAL_WSCUpdateProvider,
    REAL_WSCWriteNameSpaceOrder, REAL_WSCWriteProviderOrder,
    REAL_freeaddrinfo, REAL_getaddrinfo, REAL_getnameinfo,
    REAL_inet_ntop, REAL_inet_pton, REAL___WSAFDIsSet, REAL_WEP,
}

// ---- Thunk definitions --------------------------------------------------
//
// Each thunk is a single indirect JMP. `extern "system"` pairs with
// `#[unsafe(naked)]` so no prologue/epilogue is emitted — the compiler
// trusts us to return via the naked_asm body.

macro_rules! thunk {
    ($name:ident, $real:ident) => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub extern "system" fn $name() {
            naked_asm!(
                "jmp dword ptr [{0}]",
                sym $real,
            )
        }
    };
}

thunk!(accept, REAL_accept);
// `bind` — logging hook in lib.rs, not a thunk (would collide on symbol).
thunk!(getpeername, REAL_getpeername);
thunk!(getsockname, REAL_getsockname);
thunk!(getsockopt, REAL_getsockopt);
thunk!(htonl, REAL_htonl);
thunk!(htons, REAL_htons);
thunk!(ioctlsocket, REAL_ioctlsocket);
thunk!(inet_addr, REAL_inet_addr);
thunk!(inet_ntoa, REAL_inet_ntoa);
// `listen` — logging hook in lib.rs, not a thunk (would collide on symbol).
thunk!(ntohl, REAL_ntohl);
thunk!(ntohs, REAL_ntohs);
thunk!(recvfrom, REAL_recvfrom);
thunk!(select, REAL_select);
thunk!(sendto, REAL_sendto);
thunk!(setsockopt, REAL_setsockopt);
thunk!(shutdown, REAL_shutdown);
thunk!(socket, REAL_socket);

thunk!(FreeAddrInfoEx, REAL_FreeAddrInfoEx);
thunk!(FreeAddrInfoExW, REAL_FreeAddrInfoExW);
thunk!(FreeAddrInfoW, REAL_FreeAddrInfoW);
thunk!(GetAddrInfoExCancel, REAL_GetAddrInfoExCancel);
thunk!(GetAddrInfoExOverlappedResult, REAL_GetAddrInfoExOverlappedResult);
thunk!(GetAddrInfoExW, REAL_GetAddrInfoExW);
thunk!(GetAddrInfoW, REAL_GetAddrInfoW);
thunk!(GetHostNameW, REAL_GetHostNameW);
thunk!(GetNameInfoW, REAL_GetNameInfoW);
thunk!(InetNtopW, REAL_InetNtopW);
thunk!(InetPtonW, REAL_InetPtonW);

thunk!(WSApSetPostRoutine, REAL_WSApSetPostRoutine);
thunk!(WPUCompleteOverlappedRequest, REAL_WPUCompleteOverlappedRequest);
thunk!(WSAAccept, REAL_WSAAccept);
thunk!(WSAAddressToStringA, REAL_WSAAddressToStringA);
thunk!(WSAAddressToStringW, REAL_WSAAddressToStringW);
thunk!(WSACloseEvent, REAL_WSACloseEvent);
thunk!(WSAConnect, REAL_WSAConnect);
thunk!(WSAConnectByNameA, REAL_WSAConnectByNameA);
thunk!(WSAConnectByNameW, REAL_WSAConnectByNameW);
thunk!(WSACreateEvent, REAL_WSACreateEvent);
thunk!(WSADuplicateSocketA, REAL_WSADuplicateSocketA);
thunk!(WSADuplicateSocketW, REAL_WSADuplicateSocketW);
thunk!(WSAEnumNameSpaceProvidersA, REAL_WSAEnumNameSpaceProvidersA);
thunk!(WSAEnumNameSpaceProvidersW, REAL_WSAEnumNameSpaceProvidersW);
thunk!(WSAEnumNetworkEvents, REAL_WSAEnumNetworkEvents);
thunk!(WSAEnumProtocolsA, REAL_WSAEnumProtocolsA);

thunk!(gethostbyaddr, REAL_gethostbyaddr);
thunk!(gethostbyname, REAL_gethostbyname);
thunk!(getprotobyname, REAL_getprotobyname);
thunk!(getprotobynumber, REAL_getprotobynumber);
thunk!(getservbyname, REAL_getservbyname);
thunk!(getservbyport, REAL_getservbyport);
thunk!(gethostname, REAL_gethostname);

thunk!(WSAEnumProtocolsW, REAL_WSAEnumProtocolsW);
thunk!(WSAEventSelect, REAL_WSAEventSelect);
thunk!(WSAGetOverlappedResult, REAL_WSAGetOverlappedResult);
thunk!(WSAGetQOSByName, REAL_WSAGetQOSByName);
thunk!(WSAGetServiceClassInfoA, REAL_WSAGetServiceClassInfoA);
thunk!(WSAGetServiceClassInfoW, REAL_WSAGetServiceClassInfoW);
thunk!(WSAGetServiceClassNameByClassIdA, REAL_WSAGetServiceClassNameByClassIdA);
thunk!(WSAGetServiceClassNameByClassIdW, REAL_WSAGetServiceClassNameByClassIdW);
thunk!(WSAHtonl, REAL_WSAHtonl);
thunk!(WSAHtons, REAL_WSAHtons);
thunk!(WSAInstallServiceClassA, REAL_WSAInstallServiceClassA);
thunk!(WSAInstallServiceClassW, REAL_WSAInstallServiceClassW);
thunk!(WSAIoctl, REAL_WSAIoctl);
thunk!(WSAJoinLeaf, REAL_WSAJoinLeaf);
thunk!(WSALookupServiceBeginA, REAL_WSALookupServiceBeginA);
thunk!(WSALookupServiceBeginW, REAL_WSALookupServiceBeginW);
thunk!(WSALookupServiceEnd, REAL_WSALookupServiceEnd);
thunk!(WSALookupServiceNextA, REAL_WSALookupServiceNextA);
thunk!(WSALookupServiceNextW, REAL_WSALookupServiceNextW);
thunk!(WSANSPIoctl, REAL_WSANSPIoctl);
thunk!(WSANtohl, REAL_WSANtohl);
thunk!(WSANtohs, REAL_WSANtohs);
thunk!(WSAPoll, REAL_WSAPoll);
thunk!(WSAProviderConfigChange, REAL_WSAProviderConfigChange);
thunk!(WSARecvDisconnect, REAL_WSARecvDisconnect);
thunk!(WSARecvFrom, REAL_WSARecvFrom);
thunk!(WSARemoveServiceClass, REAL_WSARemoveServiceClass);
thunk!(WSAResetEvent, REAL_WSAResetEvent);
thunk!(WSASendDisconnect, REAL_WSASendDisconnect);
thunk!(WSASendMsg, REAL_WSASendMsg);
thunk!(WSASendTo, REAL_WSASendTo);
thunk!(WSASetEvent, REAL_WSASetEvent);
thunk!(WSASetServiceA, REAL_WSASetServiceA);
thunk!(WSASetServiceW, REAL_WSASetServiceW);
thunk!(WSASocketA, REAL_WSASocketA);
thunk!(WSASocketW, REAL_WSASocketW);
thunk!(WSAStringToAddressA, REAL_WSAStringToAddressA);
thunk!(WSAStringToAddressW, REAL_WSAStringToAddressW);
thunk!(WSAWaitForMultipleEvents, REAL_WSAWaitForMultipleEvents);
thunk!(WSCDeinstallProvider, REAL_WSCDeinstallProvider);
thunk!(WSCEnableNSProvider, REAL_WSCEnableNSProvider);

thunk!(WSAAsyncSelect, REAL_WSAAsyncSelect);
thunk!(WSAAsyncGetHostByAddr, REAL_WSAAsyncGetHostByAddr);
thunk!(WSAAsyncGetHostByName, REAL_WSAAsyncGetHostByName);
thunk!(WSAAsyncGetProtoByNumber, REAL_WSAAsyncGetProtoByNumber);
thunk!(WSAAsyncGetProtoByName, REAL_WSAAsyncGetProtoByName);
thunk!(WSAAsyncGetServByPort, REAL_WSAAsyncGetServByPort);
thunk!(WSAAsyncGetServByName, REAL_WSAAsyncGetServByName);
thunk!(WSACancelAsyncRequest, REAL_WSACancelAsyncRequest);
thunk!(WSASetBlockingHook, REAL_WSASetBlockingHook);
thunk!(WSAUnhookBlockingHook, REAL_WSAUnhookBlockingHook);
thunk!(WSAGetLastError, REAL_WSAGetLastError);
thunk!(WSASetLastError, REAL_WSASetLastError);
thunk!(WSACancelBlockingCall, REAL_WSACancelBlockingCall);
thunk!(WSAIsBlocking, REAL_WSAIsBlocking);
thunk!(WSAStartup, REAL_WSAStartup);
thunk!(WSACleanup, REAL_WSACleanup);
thunk!(WSCEnumProtocols, REAL_WSCEnumProtocols);
thunk!(WSCGetProviderInfo, REAL_WSCGetProviderInfo);
thunk!(WSCGetProviderPath, REAL_WSCGetProviderPath);
thunk!(WSCInstallNameSpace, REAL_WSCInstallNameSpace);
thunk!(WSCInstallProvider, REAL_WSCInstallProvider);
thunk!(WSCSetApplicationCategory, REAL_WSCSetApplicationCategory);
thunk!(WSCUnInstallNameSpace, REAL_WSCUnInstallNameSpace);
thunk!(WSCUpdateProvider, REAL_WSCUpdateProvider);
thunk!(WSCWriteNameSpaceOrder, REAL_WSCWriteNameSpaceOrder);
thunk!(WSCWriteProviderOrder, REAL_WSCWriteProviderOrder);

thunk!(freeaddrinfo, REAL_freeaddrinfo);
thunk!(getaddrinfo, REAL_getaddrinfo);
thunk!(getnameinfo, REAL_getnameinfo);
thunk!(inet_ntop, REAL_inet_ntop);
thunk!(inet_pton, REAL_inet_pton);

thunk!(__WSAFDIsSet, REAL___WSAFDIsSet);
thunk!(WEP, REAL_WEP);

// -------------------------------------------------------------------------
// Population.
//
// Called from `DllMain(PROCESS_ATTACH)` once. Resolves the real DLL via
// an absolute syswow64 path — a bare `"ws2_32.dll"` would return our
// own HMODULE since the PE loader caches by normalised path and we
// were loaded under that name from the exe directory. Using the full
// path gives us a distinct cache entry (different path → different
// HMODULE), so the underlying Wine builtin's `DllMain` runs cleanly
// under its own registered name and we get a valid function table.
// -------------------------------------------------------------------------

pub fn bind_all() {
    // Try 32-bit syswow64 first (correct for the 32-bit game), fall
    // back to system32 for pure-32-bit prefixes. Both are absolute
    // paths, bypassing our proxy.
    let handle = unsafe {
        let syswow = c"C:\\windows\\syswow64\\ws2_32.dll";
        let sys32 = c"C:\\windows\\system32\\ws2_32.dll";
        let h = LoadLibraryA(syswow.as_ptr() as *const u8);
        if !h.is_null() {
            h
        } else {
            LoadLibraryA(sys32.as_ptr() as *const u8)
        }
    };
    if handle.is_null() {
        // Without the real DLL every JMP would land on null — every
        // socket call would fault. There's nothing useful we can do;
        // let the pointers stay zeroed and let the first call that
        // tries to use them crash with an obvious null-JMP. Better
        // than silently misbehaving.
        return;
    }
    unsafe { resolve_all(handle) };
}

macro_rules! resolve_one {
    ($handle:expr, $slot:ident, $name:expr) => {
        if let Some(p) = GetProcAddress($handle, $name.as_ptr() as *const u8) {
            $slot = p as usize;
        }
    };
}

unsafe fn resolve_all(h: HMODULE) {
    // ---- BSD sockets we don't hook ----
    resolve_one!(h, REAL_accept, c"accept");
    // `bind`/`listen` resolve their real pointers in `real.rs` (they are
    // logging hooks, not thunks), so no slot is resolved for them here.
    resolve_one!(h, REAL_getpeername, c"getpeername");
    resolve_one!(h, REAL_getsockname, c"getsockname");
    resolve_one!(h, REAL_getsockopt, c"getsockopt");
    resolve_one!(h, REAL_htonl, c"htonl");
    resolve_one!(h, REAL_htons, c"htons");
    resolve_one!(h, REAL_ioctlsocket, c"ioctlsocket");
    resolve_one!(h, REAL_inet_addr, c"inet_addr");
    resolve_one!(h, REAL_inet_ntoa, c"inet_ntoa");
    resolve_one!(h, REAL_ntohl, c"ntohl");
    resolve_one!(h, REAL_ntohs, c"ntohs");
    resolve_one!(h, REAL_recvfrom, c"recvfrom");
    resolve_one!(h, REAL_select, c"select");
    resolve_one!(h, REAL_sendto, c"sendto");
    resolve_one!(h, REAL_setsockopt, c"setsockopt");
    resolve_one!(h, REAL_shutdown, c"shutdown");
    resolve_one!(h, REAL_socket, c"socket");

    resolve_one!(h, REAL_FreeAddrInfoEx, c"FreeAddrInfoEx");
    resolve_one!(h, REAL_FreeAddrInfoExW, c"FreeAddrInfoExW");
    resolve_one!(h, REAL_FreeAddrInfoW, c"FreeAddrInfoW");
    resolve_one!(h, REAL_GetAddrInfoExCancel, c"GetAddrInfoExCancel");
    resolve_one!(h, REAL_GetAddrInfoExOverlappedResult, c"GetAddrInfoExOverlappedResult");
    resolve_one!(h, REAL_GetAddrInfoExW, c"GetAddrInfoExW");
    resolve_one!(h, REAL_GetAddrInfoW, c"GetAddrInfoW");
    resolve_one!(h, REAL_GetHostNameW, c"GetHostNameW");
    resolve_one!(h, REAL_GetNameInfoW, c"GetNameInfoW");
    resolve_one!(h, REAL_InetNtopW, c"InetNtopW");
    resolve_one!(h, REAL_InetPtonW, c"InetPtonW");

    resolve_one!(h, REAL_WSApSetPostRoutine, c"WSApSetPostRoutine");
    resolve_one!(h, REAL_WPUCompleteOverlappedRequest, c"WPUCompleteOverlappedRequest");
    resolve_one!(h, REAL_WSAAccept, c"WSAAccept");
    resolve_one!(h, REAL_WSAAddressToStringA, c"WSAAddressToStringA");
    resolve_one!(h, REAL_WSAAddressToStringW, c"WSAAddressToStringW");
    resolve_one!(h, REAL_WSACloseEvent, c"WSACloseEvent");
    resolve_one!(h, REAL_WSAConnect, c"WSAConnect");
    resolve_one!(h, REAL_WSAConnectByNameA, c"WSAConnectByNameA");
    resolve_one!(h, REAL_WSAConnectByNameW, c"WSAConnectByNameW");
    resolve_one!(h, REAL_WSACreateEvent, c"WSACreateEvent");
    resolve_one!(h, REAL_WSADuplicateSocketA, c"WSADuplicateSocketA");
    resolve_one!(h, REAL_WSADuplicateSocketW, c"WSADuplicateSocketW");
    resolve_one!(h, REAL_WSAEnumNameSpaceProvidersA, c"WSAEnumNameSpaceProvidersA");
    resolve_one!(h, REAL_WSAEnumNameSpaceProvidersW, c"WSAEnumNameSpaceProvidersW");
    resolve_one!(h, REAL_WSAEnumNetworkEvents, c"WSAEnumNetworkEvents");
    resolve_one!(h, REAL_WSAEnumProtocolsA, c"WSAEnumProtocolsA");

    resolve_one!(h, REAL_gethostbyaddr, c"gethostbyaddr");
    resolve_one!(h, REAL_gethostbyname, c"gethostbyname");
    resolve_one!(h, REAL_getprotobyname, c"getprotobyname");
    resolve_one!(h, REAL_getprotobynumber, c"getprotobynumber");
    resolve_one!(h, REAL_getservbyname, c"getservbyname");
    resolve_one!(h, REAL_getservbyport, c"getservbyport");
    resolve_one!(h, REAL_gethostname, c"gethostname");

    resolve_one!(h, REAL_WSAEnumProtocolsW, c"WSAEnumProtocolsW");
    resolve_one!(h, REAL_WSAEventSelect, c"WSAEventSelect");
    resolve_one!(h, REAL_WSAGetOverlappedResult, c"WSAGetOverlappedResult");
    resolve_one!(h, REAL_WSAGetQOSByName, c"WSAGetQOSByName");
    resolve_one!(h, REAL_WSAGetServiceClassInfoA, c"WSAGetServiceClassInfoA");
    resolve_one!(h, REAL_WSAGetServiceClassInfoW, c"WSAGetServiceClassInfoW");
    resolve_one!(h, REAL_WSAGetServiceClassNameByClassIdA, c"WSAGetServiceClassNameByClassIdA");
    resolve_one!(h, REAL_WSAGetServiceClassNameByClassIdW, c"WSAGetServiceClassNameByClassIdW");
    resolve_one!(h, REAL_WSAHtonl, c"WSAHtonl");
    resolve_one!(h, REAL_WSAHtons, c"WSAHtons");
    resolve_one!(h, REAL_WSAInstallServiceClassA, c"WSAInstallServiceClassA");
    resolve_one!(h, REAL_WSAInstallServiceClassW, c"WSAInstallServiceClassW");
    resolve_one!(h, REAL_WSAIoctl, c"WSAIoctl");
    resolve_one!(h, REAL_WSAJoinLeaf, c"WSAJoinLeaf");
    resolve_one!(h, REAL_WSALookupServiceBeginA, c"WSALookupServiceBeginA");
    resolve_one!(h, REAL_WSALookupServiceBeginW, c"WSALookupServiceBeginW");
    resolve_one!(h, REAL_WSALookupServiceEnd, c"WSALookupServiceEnd");
    resolve_one!(h, REAL_WSALookupServiceNextA, c"WSALookupServiceNextA");
    resolve_one!(h, REAL_WSALookupServiceNextW, c"WSALookupServiceNextW");
    resolve_one!(h, REAL_WSANSPIoctl, c"WSANSPIoctl");
    resolve_one!(h, REAL_WSANtohl, c"WSANtohl");
    resolve_one!(h, REAL_WSANtohs, c"WSANtohs");
    resolve_one!(h, REAL_WSAPoll, c"WSAPoll");
    resolve_one!(h, REAL_WSAProviderConfigChange, c"WSAProviderConfigChange");
    resolve_one!(h, REAL_WSARecvDisconnect, c"WSARecvDisconnect");
    resolve_one!(h, REAL_WSARecvFrom, c"WSARecvFrom");
    resolve_one!(h, REAL_WSARemoveServiceClass, c"WSARemoveServiceClass");
    resolve_one!(h, REAL_WSAResetEvent, c"WSAResetEvent");
    resolve_one!(h, REAL_WSASendDisconnect, c"WSASendDisconnect");
    resolve_one!(h, REAL_WSASendMsg, c"WSASendMsg");
    resolve_one!(h, REAL_WSASendTo, c"WSASendTo");
    resolve_one!(h, REAL_WSASetEvent, c"WSASetEvent");
    resolve_one!(h, REAL_WSASetServiceA, c"WSASetServiceA");
    resolve_one!(h, REAL_WSASetServiceW, c"WSASetServiceW");
    resolve_one!(h, REAL_WSASocketA, c"WSASocketA");
    resolve_one!(h, REAL_WSASocketW, c"WSASocketW");
    resolve_one!(h, REAL_WSAStringToAddressA, c"WSAStringToAddressA");
    resolve_one!(h, REAL_WSAStringToAddressW, c"WSAStringToAddressW");
    resolve_one!(h, REAL_WSAWaitForMultipleEvents, c"WSAWaitForMultipleEvents");
    resolve_one!(h, REAL_WSCDeinstallProvider, c"WSCDeinstallProvider");
    resolve_one!(h, REAL_WSCEnableNSProvider, c"WSCEnableNSProvider");

    resolve_one!(h, REAL_WSAAsyncSelect, c"WSAAsyncSelect");
    resolve_one!(h, REAL_WSAAsyncGetHostByAddr, c"WSAAsyncGetHostByAddr");
    resolve_one!(h, REAL_WSAAsyncGetHostByName, c"WSAAsyncGetHostByName");
    resolve_one!(h, REAL_WSAAsyncGetProtoByNumber, c"WSAAsyncGetProtoByNumber");
    resolve_one!(h, REAL_WSAAsyncGetProtoByName, c"WSAAsyncGetProtoByName");
    resolve_one!(h, REAL_WSAAsyncGetServByPort, c"WSAAsyncGetServByPort");
    resolve_one!(h, REAL_WSAAsyncGetServByName, c"WSAAsyncGetServByName");
    resolve_one!(h, REAL_WSACancelAsyncRequest, c"WSACancelAsyncRequest");
    resolve_one!(h, REAL_WSASetBlockingHook, c"WSASetBlockingHook");
    resolve_one!(h, REAL_WSAUnhookBlockingHook, c"WSAUnhookBlockingHook");
    resolve_one!(h, REAL_WSAGetLastError, c"WSAGetLastError");
    resolve_one!(h, REAL_WSASetLastError, c"WSASetLastError");
    resolve_one!(h, REAL_WSACancelBlockingCall, c"WSACancelBlockingCall");
    resolve_one!(h, REAL_WSAIsBlocking, c"WSAIsBlocking");
    resolve_one!(h, REAL_WSAStartup, c"WSAStartup");
    resolve_one!(h, REAL_WSACleanup, c"WSACleanup");
    resolve_one!(h, REAL_WSCEnumProtocols, c"WSCEnumProtocols");
    resolve_one!(h, REAL_WSCGetProviderInfo, c"WSCGetProviderInfo");
    resolve_one!(h, REAL_WSCGetProviderPath, c"WSCGetProviderPath");
    resolve_one!(h, REAL_WSCInstallNameSpace, c"WSCInstallNameSpace");
    resolve_one!(h, REAL_WSCInstallProvider, c"WSCInstallProvider");
    resolve_one!(h, REAL_WSCSetApplicationCategory, c"WSCSetApplicationCategory");
    resolve_one!(h, REAL_WSCUnInstallNameSpace, c"WSCUnInstallNameSpace");
    resolve_one!(h, REAL_WSCUpdateProvider, c"WSCUpdateProvider");
    resolve_one!(h, REAL_WSCWriteNameSpaceOrder, c"WSCWriteNameSpaceOrder");
    resolve_one!(h, REAL_WSCWriteProviderOrder, c"WSCWriteProviderOrder");

    resolve_one!(h, REAL_freeaddrinfo, c"freeaddrinfo");
    resolve_one!(h, REAL_getaddrinfo, c"getaddrinfo");
    resolve_one!(h, REAL_getnameinfo, c"getnameinfo");
    resolve_one!(h, REAL_inet_ntop, c"inet_ntop");
    resolve_one!(h, REAL_inet_pton, c"inet_pton");

    resolve_one!(h, REAL___WSAFDIsSet, c"__WSAFDIsSet");
    resolve_one!(h, REAL_WEP, c"WEP");

    // `handle` stays loaded for the process lifetime; the loader will
    // clean it up on exit. Silence the unused import from the `c_void`
    // path we kept around in case future hooks need raw pointers.
    let _ = h as *mut c_void;
}

/// Exposed so `real::bind()` (the six hooks' pointer table) can reuse
/// the same resolution and not re-load the DLL twice.
pub fn system_ws2_32_handle() -> Option<HMODULE> {
    unsafe {
        let syswow = c"C:\\windows\\syswow64\\ws2_32.dll";
        let sys32 = c"C:\\windows\\system32\\ws2_32.dll";
        let h = LoadLibraryA(syswow.as_ptr() as *const u8);
        if !h.is_null() {
            return Some(h);
        }
        let h = LoadLibraryA(sys32.as_ptr() as *const u8);
        if !h.is_null() { Some(h) } else { None }
    }
}

// Silence the placeholder macro that's only there as a design note.
#[allow(unused_macros)]
macro_rules! paste_concat_ident {
    ($($tt:tt)*) => {};
}
