# Garlemald Client

## Contents

1. [NOTICE](#notice)
2. [Introduction](#introduction)
   - [What it does](#what-it-does)
   - [What's different from Seventh Umbral Launcher](#whats-different-from-seventh-umbral-launcher)
3. [Requirements](#requirements)
   - [Running the launcher](#running-the-launcher)
   - [Running the game](#running-the-game)
4. [Building from source](#building-from-source)
5. [Configuration](#configuration)
   - [Server list](#server-list)
   - [User preferences](#user-preferences)
   - [Bundled starter config](#bundled-starter-config)
6. [Running](#running)
   - [First-time flow](#first-time-flow)
   - [Platform notes](#platform-notes)
7. [Packaging](#packaging)
8. [Layout](#layout)
9. [Logging](#logging)
10. [Testing](#testing)
11. [Troubleshooting](#troubleshooting)
12. [References](#references)

## NOTICE

This launcher drives the **FINAL FANTASY XIV v1.23b** client (the original
1.0 iteration of the game), not A Realm Reborn. The expected client
version is `2012.09.19.0001`; any other version will either be brought
to 1.23b by the built-in patcher or rejected by the launcher.

## Introduction

Garlemald Client is a cross-platform Rust rewrite of the Windows-only
[Seventh Umbral Launcher](https://github.com/Meteor-Project/SeventhUmbral).
It handles everything that launcher did - game install detection, patch
download and apply, login, server-address injection into the game
binary - but runs on macOS (including Apple Silicon), Linux, and
Windows out of the same codebase.

On macOS and Linux it also manages its own
[Wine](https://www.winehq.org/) runtime and prefix automatically. There
is nothing to install beyond the launcher itself.

### What it does

Run-time responsibilities:

- **Detect the installed FFXIV 1.x client** on every supported platform,
  via registry (Windows) or filesystem heuristics (macOS / Linux).
- **Patch the client** from `2010.09.18.0000` up to
  `2012.09.19.0001` by downloading the official `.patch` files from a
  remote mirror, verifying them via CRC32, and applying them via the
  ZiPatch format decoder.
- **Inject the lobby-server hostname** into the game binary at launch
  time using a PE-section rewrite, so the same FFXIV 1.23b binary
  can point at any private server without client-side configuration.
- **Run the official login flow** (Seventh Umbral / private-server
  variants) inside an embedded WebView, extract the session token,
  and hand it to the game binary via the command-line argument format
  1.23b expects.
- **Manage the Wine prefix** on non-Windows platforms: download a
  known-good Wine runtime, bootstrap a prefix inside the user's data
  directory, and use it only for launching `ffxivgame.exe`.

The launcher GUI is egui via eframe. The login WebView is wry (via tao).

### What's different from Seventh Umbral Launcher

| Aspect              | Seventh Umbral (C#)           | Garlemald Client (Rust)                                   |
|---------------------|-------------------------------|-----------------------------------------------------------|
| Platform            | Windows only (.NET Framework) | macOS (incl. Apple Silicon), Linux, Windows               |
| GUI                 | WinForms                      | egui / eframe                                             |
| Login web view      | IE trident control            | wry / tao cross-platform WebView                          |
| Wine runtime        | not applicable                | managed automatically under the user's data directory     |
| Server list format  | `servers.xml`                 | same XML format, extended with a localhost default        |
| Preferences         | registry                      | TOML under the platform config dir + optional repo bundle |
| Patcher             | inline in launcher            | same set of steps, verified with CRC32 + ZiPatch decode   |

## Requirements

### Running the launcher

| Component      | Minimum       | Notes                                                                                |
|----------------|---------------|--------------------------------------------------------------------------------------|
| Rust toolchain | 1.95.0        | Pinned in `rust-toolchain.toml`; `rustup` installs automatically.                    |
| C compiler     | any           | Required transitively by the WebView crates on some platforms.                       |
| Disk           | ~1 GB         | Launcher binary, Wine runtime (non-Windows), downloaded patch files.                 |
| OS             | see above     | On macOS, the launcher uses its own Wine runtime - no Apple Silicon emulation layer. |

Extra tooling needed by some targets:

- **Linux:** `gtk` / `webkit2gtk` runtime libraries for the login
  WebView (exact package names depend on distribution - `libgtk-3-dev`
  and `libwebkit2gtk-4.1-dev` on Debian / Ubuntu).
- **macOS:** Xcode command-line tools (`xcode-select --install`) for
  the system linker.
- **Windows:** MSVC Build Tools if you want to build from source.

### Running the game

| Component                | Version            |
|--------------------------|--------------------|
| Final Fantasy XIV 1.x    | anything from 1.0 - 1.23b; the launcher patches forward |
| Sibling `garlemald-server` running | lobby + world + map processes reachable at the configured server address |

The launcher will bring an older client up to `2012.09.19.0001`. Once
up to date it will not try to re-patch.

## Building from source

```bash
git clone https://github.com/swstegall/garlemald-client.git
cd garlemald-client
cargo build --release
```

Run it directly out of the target directory:

```bash
./target/release/garlemald-client
```

For day-to-day development, `cargo run` works too:

```bash
cargo run
```

## Configuration

Garlemald Client pulls configuration from three layers, in order of
precedence (first hit wins):

1. **Runtime UI state.** What the user selects in the GUI and presses
   "Save" on.
2. **Per-user preferences file.** Platform config dir, e.g.
   `~/.config/garlemald-client/preferences.toml` on Linux /
   macOS, `%APPDATA%\garlemald-client\preferences.toml` on Windows.
3. **Bundled starter config.** `configs/garlemald-client.toml` in the
   repo root. Only consulted when the per-user file does not yet
   exist, so fresh clones boot with localhost defaults matching the
   sibling garlemald-server.

### Server list

The launcher's server dropdown is fed by
`src/servers/default_servers.xml`, which ships two entries by default:

```xml
<Servers>
    <Server Name="Localhost" Address="127.0.0.1" LoginUrl="" />
    <Server Name="Van Darnus Server"
            Address="vandarnus.seventhumbral.org"
            LoginUrl="https://vandarnus.seventhumbral.org/login.php" />
</Servers>
```

`Localhost` sorts first alphabetically and is the default pick on first
run, matching the sibling `garlemald-server`'s localhost bind. Add more
entries by editing the per-user `servers.xml` (placed next to
`preferences.toml` in the platform config dir). An empty `LoginUrl`
means the launcher skips its WebView-based login step and expects the
user to paste a session token into the Dev Session field instead -
handy when running against a Garlemald server that has no login web
frontend.

### User preferences

The per-user TOML file captures what the GUI lets the user pick:

```toml
[launcher]
server_name = "Localhost"
server_address = "127.0.0.1"
game_location = "/Users/you/Games/FFXIV 1.x"
wine_runtime_dir = "/Users/you/Library/Application Support/garlemald-client/wine"
patch_download_dir = "/Users/you/Library/Application Support/garlemald-client/ffxiv_patches"
```

Every field is optional. Unset values fall back to platform-specific
auto-detection (game install via registry / heuristics; Wine runtime
and patch dir under the platform data dir).

### Bundled starter config

`configs/garlemald-client.toml` ships localhost defaults that match the
sibling `garlemald-server`:

```toml
[launcher]
server_name = "Localhost"
server_address = "127.0.0.1"
```

The launcher only reads this on first run (no per-user file yet). After
the first **Save** press, the platform-path file takes over and the
bundled one is no longer consulted. Delete the platform file to fall
back to bundled defaults.

## Running

### First-time flow

1. Start the sibling server (`../garlemald-server`) and create a
   session row (see its README).
2. Launch the client:
   ```bash
   cargo run --release
   ```
3. On first boot the Bundled starter config is applied, so `Localhost`
   is already the selected server.
4. Open **Game Settings** (settings gear) and:
   - set **Game Location** to your FFXIV 1.x install.
   - if you are on macOS / Linux, leave **Wine Runtime** unset to let
     the launcher download + manage a known-good Wine automatically.
5. If the launcher reports the install is out of date, click
   **Download Patches**. The patcher will fetch every missing patch,
   CRC-verify it, and apply it via the ZiPatch decoder.
6. Click **Login**. On a server that has a `LoginUrl`, the embedded
   WebView opens and you authenticate there. On a server with empty
   `LoginUrl` (the Localhost default), paste the 56-character session
   id you inserted into `sessions` on the server side into the
   **Dev Session** field.
7. Click **Launch Game**. The launcher spawns `ffxivgame.exe` via the
   platform adapter (Wine on non-Windows), patches the lobby hostname
   into the PE at spawn time, and hands over the session id.

### Platform notes

**macOS.** The launcher detects a FFXIV install anywhere on disk
containing a `drive_c/` directory with the game under it (previous
CrossOver bottles, Whisky prefixes, or manual Wine installs all work).
The managed Wine runtime goes under
`~/Library/Application Support/garlemald-client/wine/`. Signed binaries
are ad-hoc by default; use `scripts/package-macos.sh --sign` for a
distributable `.app` bundle.

**Linux.** Expects `gtk3` + `webkit2gtk` at runtime. The managed Wine
prefix goes under `~/.local/share/garlemald-client/wine/`.

**Windows.** Reads the install path from the registry
(`HKLM\SOFTWARE\SquareEnix\FinalFantasyXIV\1.0`) when available, falls
back to the standard Program Files locations. No Wine is used - the
game is launched directly.

## Packaging

### macOS .app bundle

```bash
scripts/package-macos.sh              # host arch only, ad-hoc signed
scripts/package-macos.sh --universal  # x86_64 + aarch64 fat binary
scripts/package-macos.sh --sign 'Developer ID Application: ...'
```

Output lands at `target/macos-app/Garlemald Client.app`. The target dir
is wiped before each build, so re-runs are safe.

### Linux

No first-class packaging target yet. `cargo build --release` produces
a single binary that only needs `gtk3` / `webkit2gtk` available at
runtime.

### Windows

`cargo build --release` produces a single `garlemald-client.exe`.

## Layout

```
garlemald-client/
|-- Cargo.toml              crate manifest
|-- rust-toolchain.toml     pinned toolchain (1.95.0)
|-- configs/                bundled starter configs
|   `-- garlemald-client.toml    localhost defaults (first-boot only)
|-- examples/
|   `-- apply_patch.rs      standalone ZiPatch apply example
|-- scripts/
|   `-- package-macos.sh    .app bundler
|-- src/
|   |-- main.rs             entry point (calls lib::run)
|   |-- lib.rs              wires modules + `run()`
|   |-- version.rs          APP_NAME, APP_VERSION, FFXIV_*_VERSION constants
|   |-- app/                egui launcher window + patcher + settings modals
|   |-- config/             preferences.toml + platform paths
|   |-- crypto/             command-line encryption for the client handoff
|   |-- launcher/           PE patch + game spawn
|   |-- login/              embedded WebView + subprocess login flow
|   |-- patch_format/       ZiPatch decoder
|   |-- patcher/            download manager, manifest, verification, worker
|   |-- platform/           per-OS install detection + launch (windows / macos / linux / wine)
|   `-- servers/            default_servers.xml + parser
`-- target/                 build artefacts (gitignored)
```

## Logging

Uses `env_logger`. Control via `RUST_LOG`:

```bash
RUST_LOG=debug cargo run
RUST_LOG=garlemald_client=trace,info cargo run --release
```

Log output is plain ASCII with no ANSI colour escapes, so redirecting to
a file (`cargo run 2>&1 | tee run.log`) produces a clean text file.

## Testing

```bash
cargo test
```

Unit tests cover the ZiPatch decoder, patch manifest parsing, server
list parsing, preferences roundtripping, and platform-specific path
helpers (macOS Wine prefix derivation, etc.).

## Troubleshooting

- **"could not resolve platform-specific project directories."** The
  launcher needs a user home dir. Running as a bare-UID service
  account without `$HOME` will fail at startup; set `HOME` or run as a
  normal user.
- **Login WebView is blank on Linux.** Confirm `webkit2gtk-4.1` is
  installed. On some distributions the library is split into `-core`
  and `-settings` packages and only one is pulled in by default.
- **"Game Version mismatch" after patching.** Delete
  `<game_location>/game.ver` and re-run the patcher. The file is how
  the launcher detects the currently installed version.
- **macOS Gatekeeper refuses to open the .app.** Use the unsigned
  build flow: right-click the `.app` in Finder and choose **Open**
  the first time. Or build signed with
  `scripts/package-macos.sh --sign '<your identity>'`.
- **"connection refused" from the game.** The server is not running or
  is bound to a different interface than the one the PE patch wrote
  into the binary. Check `server_address` in the launcher matches the
  lobby-server bind address, and that the sibling `garlemald-server`
  is up (`tail -f ../garlemald-server/logs/*.log`).

## References

- Upstream Seventh Umbral Launcher:
  <https://github.com/Meteor-Project/SeventhUmbral>
- FFXIV 1.x PE patch format and ZiPatch notes:
  <http://ffxivclassic.fragmenterworks.com/wiki/>
- Sibling server: `../garlemald-server/` (see its README for
  session-token setup, which is what the launcher will ask you for on
  a Localhost login).
