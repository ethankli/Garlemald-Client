// garlemald-client — cross-platform launcher for FINAL FANTASY XIV 1.x private servers
// Copyright (C) 2026  Samuel Stegall
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Build glue.
//!
//! Embeds `assets/icon.ico` into the PE resource table on Windows so
//! Explorer, the taskbar, and Alt-Tab all pick up the Garlemald logo
//! without the caller having to ship a sidecar `.ico` next to the exe.
//!
//! On non-Windows targets this is a no-op; the macOS .app bundle picks
//! up `assets/icon.icns` via `scripts/package-macos.sh`.

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let icon = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    if !icon.exists() {
        // Don't hard-fail: the icon is optional for local dev builds.
        // Regenerate with `scripts/generate-icons.py` to get it back.
        println!(
            "cargo:warning=assets/icon.ico missing — Windows build will have no embedded icon"
        );
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon.to_str().expect("icon path is not valid UTF-8"));
    if let Err(err) = res.compile() {
        println!("cargo:warning=failed to embed Windows icon: {err}");
    }
}
