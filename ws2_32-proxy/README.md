# ws2_32-proxy

A DLL-hijack proxy for `ws2_32.dll`. When copied next to `ffxivgame.patched.exe`
the Windows loader picks this DLL up before the real `C:\windows\system32\ws2_32.dll`,
giving us a chance to tap every winsock `send` / `recv` call the 1.23b client
makes. Everything not explicitly hooked is transparently forwarded to the real
DLL via PE `FORWARD` export entries.

Purpose: supply ground-truth byte-level client-side network traces for
debugging the garlemald-server protocol port. We already capture server-side
packets via `GARLEMALD_PACKET_LOG_DIR`; this fills in the client side so we
can verify TCP framing assumptions and cross-check send/recv timing against
Lua-error reports.

## Hooked functions

- `send` / `WSASend`  → log + forward
- `recv` / `WSARecv`  → log + forward
- `connect`           → log + forward
- `closesocket`       → log + forward
- `bind`              → log + forward
- `listen`            → log + forward

`bind` and `listen` are hooked to answer, by measurement rather than
assumption, whether the 1.23b client binds any fixed *local* ports — the
client makes outbound connections to the server, so a local bind would be
unusual, and a clash with another process would be a client-side failure
indistinguishable (from the server's side) from a blocked port. When
either call fails, its record carries a `WSAGetLastError=<n>`
continuation line; a port clash is `WSAEADDRINUSE`, 10048.

Every other ws2_32 export passes through untouched via PE forwarding.

## Log file

On first use (lazy init at the first call), creates
`<game_dir>/ws2_32-trace.log` in append mode. Each record:

```
<ISO-timestamp> <TID> <fn> socket=<handle> len=<n> [err=<errno>]
  <xxd-style hex dump>
```

## Build

Requires cross-compilation from macOS:

```
brew install mingw-w64
rustup target add i686-pc-windows-gnu
cd garlemald-client/ws2_32-proxy
cargo build --release --target i686-pc-windows-gnu
```

Output: `target/i686-pc-windows-gnu/release/ws2_32.dll`.

## Deployment

`garlemald-client`'s Wine launcher copies this DLL next to
`ffxivgame.patched.exe` when **Developer Settings → Enable winsock tracing**
is checked. Unchecking the box deletes it so the real DLL loads.

## Safety

The hooks run on the game's thread for every socket call. They must not
panic (compiled with `panic = "abort"`) and must not block — all logging is
buffered + line-flushed to avoid bouncing through a mutex on every call.
