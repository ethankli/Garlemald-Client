# Garlemald Client

A cross-platform launcher for **FINAL FANTASY XIV v1.23b** (the original
1.0 iteration of the game, not A Realm Reborn). It detects an installed
1.x client, patches it up to `2012.09.19.0001`, runs the login flow,
and launches `ffxivgame.exe` against a private server — on macOS
(including Apple Silicon), Linux, and Windows from the same codebase.
On macOS and Linux it also downloads and manages its own Wine runtime,
so there is nothing to install beyond the launcher itself.

> Created with [Claude](https://claude.ai/).

## Attribution and licensing

Garlemald Client derives from upstream projects under copyleft and
permissive licenses. See [`NOTICE.md`](NOTICE.md) for attribution to
Project Meteor Server, Seventh Umbral, and the wider 1.0
preservationist community, and [`LICENSE.md`](LICENSE.md) for the full
terms of the GNU Affero General Public License, version 3 or later,
under which this project is distributed.

## Build

Requires Rust 1.95.0 (pinned in `rust-toolchain.toml`; `rustup` installs
it automatically on first build). On Linux, `gtk3` and `webkit2gtk-4.1`
runtime libraries are needed for the login WebView.

```bash
cargo build --release
cargo run --release
```

For a distributable macOS `.app` bundle:

```bash
scripts/package-macos.sh               # host arch, ad-hoc signed
scripts/package-macos.sh --universal   # x86_64 + aarch64 fat binary
```

## Sister projects

- [**Garlemald-Server**](https://github.com/swstegall/Garlemald-Server)
  — the Rust FFXIV 1.23b server (lobby / world / map) this launcher is
  designed to connect to.
- [**XIV-1.0-Apple-Silicon-Installer**](https://github.com/swstegall/XIV-1.0-Apple-Silicon-Installer)
  — helper for getting a working FFXIV 1.x install on Apple Silicon
  Macs, which Garlemald Client can then detect and drive.

## Community

Questions, bug reports, or just want to talk to the developer about the
project? Join the Discord:

<https://discord.gg/CVjwWs6jnX>
