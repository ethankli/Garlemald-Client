//! Build glue.
//!
//! Points the MinGW linker at our `ws2_32.def` so the resulting DLL
//! carries the forwarded exports alongside the eight we define in Rust.
//! Without this the DLL would only expose `send`/`recv`/`WSASend`/
//! `WSARecv`/`connect`/`closesocket` and the game would fail to load
//! the first time it tried to resolve any of the dozens of other
//! ws2_32 imports it needs (`WSAStartup`, `htons`, `inet_addr`, etc.).
//!
//! The `.def` path is passed as a linker positional argument. MinGW's
//! `ld` (and by extension `dlltool`, which post-processes the .def)
//! accepts a `.def` file directly in its input list and emits the
//! forward entries into the PE export directory.

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    // Only emit the linker flag when cross-compiling to Windows. Building
    // on the host for `cargo check` doesn't need the .def (and would
    // fail on an unrelated linker).
    if !target.contains("windows") {
        return;
    }

    let def = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ws2_32.def");
    // `cargo:rerun-if-changed` so edits to the .def trigger a rebuild.
    println!("cargo:rerun-if-changed={}", def.display());

    // Pass the .def file as a linker input. The flag form is what
    // mingw-ld understands directly; LLD accepts it too.
    println!("cargo:rustc-link-arg={}", def.display());

    // `--kill-at` strips the stdcall `@N` suffix from decorated export
    // names so our Rust `pub extern "system" fn send(...)` shows up in
    // the PE export table as `send` (matching what importers expect and
    // what the .def forward entries reference) rather than `send@16`.
    // Without this the DLL loads but none of our hooks ever resolve
    // because the importer looks up the plain name.
    println!("cargo:rustc-link-arg=-Wl,--kill-at");

    // The prebuilt libstd for `i686-pc-windows-gnu` was compiled with
    // panic=unwind, so a few rarely-taken landing pads reference
    // `_Unwind_Resume` from libgcc's exception runtime. We provide a
    // stub in `lib.rs` rather than linking a third-party unwind
    // library — Homebrew's MinGW ships only the SJLJ variant of
    // libgcc_s here (`libgcc_s_sjlj-1.dll`) which doesn't export
    // `_Unwind_Resume` under the name rustc's libstd expects.
}
