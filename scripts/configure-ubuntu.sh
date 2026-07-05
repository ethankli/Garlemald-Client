#!/usr/bin/env bash
# Install the system dependencies Garlemald Client needs to build and run on
# Ubuntu / Debian, skipping anything already present.
#
# It installs the same native dev libraries CI uses (see docs/dev-environment.md):
# the GTK3 / WebKit2GTK-4.1 stack the login WebView (wry/tao) links against, the
# GL / X11 / Wayland libraries the eframe/egui window needs, plus the build
# toolchain (build-essential, pkg-config). Wine and the Rust toolchain are only
# *checked* and reported — not force-installed — so this script won't clobber a
# WineHQ install or a rustup toolchain you already manage.
#
# Usage:
#   scripts/configure-ubuntu.sh [--dry-run] [--install-wine] [-h|--help]
#
#   -n, --dry-run     List what's missing and exit without installing anything.
#       --install-wine  Also apt-install the distro `wine` package when no `wine`
#                       is on PATH (skipped when Wine is already present, e.g.
#                       from WineHQ).
#
# Uses sudo automatically when not run as root.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=0
INSTALL_WINE=0

say()  { printf '==> %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Install the system dependencies Garlemald Client needs to build and run on
Ubuntu / Debian, skipping anything already present.

Usage:
  scripts/configure-ubuntu.sh [--dry-run] [--install-wine] [-h|--help]

  -n, --dry-run     List what's missing and exit without installing anything.
      --install-wine  Also apt-install the distro `wine` package when no `wine`
                      is on PATH (skipped when Wine is already present).

Uses sudo automatically when not run as root.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -n|--dry-run)   DRY_RUN=1; shift ;;
        --install-wine) INSTALL_WINE=1; shift ;;
        -h|--help)      usage; exit 0 ;;
        *) die "unknown flag: $1 (see --help)" ;;
    esac
done

# --- environment guards -----------------------------------------------------

if ! command -v apt-get >/dev/null 2>&1; then
    die "this script targets Debian/Ubuntu (needs apt-get). On another distro,
       install the equivalents of the packages in docs/dev-environment.md."
fi

# Build the sudo prefix once: empty when we're already root, `sudo` otherwise.
SUDO=""
if [[ "$(id -u)" -ne 0 ]]; then
    if command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
    else
        die "not running as root and sudo is unavailable; re-run as root."
    fi
fi

# --- the dependency set (mirrors docs/dev-environment.md / CI) --------------

REQUIRED_PKGS=(
    # Build toolchain
    build-essential
    pkg-config
    # Login WebView (wry/tao): WebKit2GTK 4.1 ABI (the libsoup3 generation)
    libwebkit2gtk-4.1-dev
    libjavascriptcoregtk-4.1-dev
    libsoup-3.0-dev
    # GTK3 — also provides glib-2.0 / gobject-2.0 / gio-2.0 / gdk / pango / cairo / atk
    libgtk-3-dev
    # Native dialogs (rfd), tray/appindicator, and SVG icon rendering
    libxdo-dev
    libayatana-appindicator3-dev
    librsvg2-dev
    # eframe glow + winit backends: OpenGL, X11, and Wayland
    libgl1-mesa-dev
    libxkbcommon-dev
    libwayland-dev
    libxcursor-dev
    libxrandr-dev
    libxi-dev
)

# Is the package installed and configured?
pkg_installed() {
    dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q 'install ok installed'
}

# Does apt have an installable candidate for it? (Guards against a package name
# that drifted between Ubuntu releases so one bad name can't fail the whole run.)
pkg_available() {
    local cand
    cand="$(apt-cache policy "$1" 2>/dev/null | awk -F': ' '/Candidate:/{print $2; exit}')"
    [[ -n "$cand" && "$cand" != "(none)" ]]
}

say "Checking ${#REQUIRED_PKGS[@]} required packages"

missing=()
unavailable=()
for p in "${REQUIRED_PKGS[@]}"; do
    if pkg_installed "$p"; then
        continue
    fi
    if pkg_available "$p"; then
        missing+=("$p")
    else
        unavailable+=("$p")
    fi
done

if [[ ${#unavailable[@]} -gt 0 ]]; then
    warn "no apt candidate for: ${unavailable[*]}"
    warn "the package names may differ on this Ubuntu release; install the"
    warn "equivalents by hand (see docs/dev-environment.md)."
fi

# --- install (or, with --dry-run, just report) ------------------------------

if [[ ${#missing[@]} -eq 0 ]]; then
    say "All build dependencies already present — nothing to install."
else
    say "Missing (${#missing[@]}): ${missing[*]}"
    if [[ $DRY_RUN -eq 1 ]]; then
        info "(dry run) would run: ${SUDO:+$SUDO }apt-get install -y ${missing[*]}"
    else
        say "Updating package lists"
        $SUDO apt-get update
        say "Installing missing packages"
        $SUDO apt-get install -y "${missing[@]}"
        say "Installed: ${missing[*]}"
    fi
fi

# --- Wine (runtime for the game itself) -------------------------------------

# Detect the `wine` *command*, not a distro package: WineHQ installs (winehq-stable)
# provide `wine` without the distro `wine`/`wine64` packages, and force-installing
# the distro package on top of WineHQ would conflict.
if command -v wine >/dev/null 2>&1; then
    say "Wine present: $(wine --version 2>/dev/null || echo 'version unknown')"
elif [[ $INSTALL_WINE -eq 1 ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
        info "(dry run) would run: ${SUDO:+$SUDO }apt-get install -y wine"
    else
        say "Installing distro wine (--install-wine)"
        $SUDO apt-get install -y wine
    fi
else
    warn "no 'wine' on PATH — the launcher builds and runs, but launching the"
    warn "game needs Wine 7+. Install the distro package with --install-wine, or"
    warn "WineHQ stable (recommended, newer): https://wiki.winehq.org/Ubuntu"
fi

# --- Rust toolchain (advisory only) -----------------------------------------

if command -v cargo >/dev/null 2>&1; then
    say "Rust present: $(cargo --version 2>/dev/null || echo 'version unknown')"
else
    warn "no 'cargo' on PATH — install rustup, then the pinned toolchain from"
    warn "rust-toolchain.toml installs automatically on first build:"
    warn "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

# --- verify pkg-config can now see the key libraries ------------------------

say "Verifying pkg-config can resolve the core libraries"
verify_ok=1
for mod in glib-2.0 gtk+-3.0 webkit2gtk-4.1; do
    if pkg-config --exists "$mod" 2>/dev/null; then
        info "ok   $mod ($(pkg-config --modversion "$mod" 2>/dev/null))"
    else
        info "MISS $mod"
        verify_ok=0
    fi
done

echo ""
if [[ $verify_ok -eq 1 ]]; then
    say "Done. Build and run with:  cargo run --release"
else
    if [[ $DRY_RUN -eq 1 ]]; then
        say "Dry run complete — re-run without --dry-run to install the above."
    else
        warn "some libraries still don't resolve; see the MISS lines above."
        exit 1
    fi
fi
