#!/usr/bin/env bash
# Package garlemald-client as a macOS .app bundle.
#
# Usage:
#   scripts/package-macos.sh [--universal] [--sign IDENTITY] [--no-sign]
#
#   --universal     Build a fat binary (aarch64 + x86_64). Default is the
#                   host architecture only.
#   --sign IDENT    Sign with the given codesign identity (e.g. a
#                   "Developer ID Application: …" certificate). Default is
#                   an ad-hoc signature, which keeps Gatekeeper quiet for
#                   local launches but is not valid for distribution.
#   --no-sign       Skip codesigning entirely.
#
# Output: target/macos-app/Garlemald Client.app
#
# Re-run-safe: the target dir is wiped before each build.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_DISPLAY_NAME="Garlemald Client"
BINARY_NAME="garlemald-client"
BUNDLE_ID="me.stegall.garlemald-client"

UNIVERSAL=0
SIGN_IDENTITY="-" # ad-hoc by default; see `man codesign`
NO_SIGN=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --universal) UNIVERSAL=1; shift ;;
        --sign)
            if [[ $# -lt 2 ]]; then
                echo "--sign requires an identity argument" >&2
                exit 2
            fi
            SIGN_IDENTITY="$2"
            shift 2
            ;;
        --no-sign) NO_SIGN=1; shift ;;
        -h|--help)
            sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown flag: $1" >&2
            echo "see --help" >&2
            exit 2
            ;;
    esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: this script only runs on macOS" >&2
    exit 1
fi

cd "$PROJECT_DIR"

# Pull the package version out of Cargo.toml. Anchoring on ^version= avoids
# matching dependency tables like `serde = { version = "1" }`.
VERSION="$(awk -F'"' '/^version[[:space:]]*=/ {print $2; exit}' Cargo.toml)"
if [[ -z "$VERSION" ]]; then
    echo "error: could not parse version from Cargo.toml" >&2
    exit 1
fi

APP_DIR="$PROJECT_DIR/target/macos-app/${APP_DISPLAY_NAME}.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RES_DIR="$CONTENTS_DIR/Resources"

echo "==> Packaging $APP_DISPLAY_NAME v$VERSION"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RES_DIR"

if [[ "$UNIVERSAL" == "1" ]]; then
    echo "==> Building aarch64-apple-darwin"
    rustup target add aarch64-apple-darwin >/dev/null 2>&1 || true
    cargo build --release --target aarch64-apple-darwin

    echo "==> Building x86_64-apple-darwin"
    rustup target add x86_64-apple-darwin >/dev/null 2>&1 || true
    cargo build --release --target x86_64-apple-darwin

    echo "==> Combining into a universal binary"
    lipo -create \
        -output "$MACOS_DIR/$BINARY_NAME" \
        "$PROJECT_DIR/target/aarch64-apple-darwin/release/$BINARY_NAME" \
        "$PROJECT_DIR/target/x86_64-apple-darwin/release/$BINARY_NAME"
else
    echo "==> cargo build --release"
    cargo build --release
    cp "$PROJECT_DIR/target/release/$BINARY_NAME" "$MACOS_DIR/$BINARY_NAME"
fi

chmod +x "$MACOS_DIR/$BINARY_NAME"

echo "==> Writing Info.plist"
cat > "$CONTENTS_DIR/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${APP_DISPLAY_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_DISPLAY_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.games</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <!--
      The embedded login flow uses WKWebView (via wry). Our login endpoints
      can be plain HTTP for localhost or self-hosted private servers, and ATS
      blocks those by default in bundled apps, which shows up as a blank white
      webview. Limit the relaxation to web content only.
    -->
    <key>NSAppTransportSecurity</key>
    <dict>
        <key>NSAllowsArbitraryLoadsInWebContent</key>
        <true/>
    </dict>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>
            <string>FFXIV login callback</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>ffxiv</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
EOF

# Optional custom icon — drop an icon.icns at assets/icon.icns (relative to
# the crate root) and it will be picked up automatically.
if [[ -f "$PROJECT_DIR/assets/icon.icns" ]]; then
    echo "==> Embedding assets/icon.icns"
    cp "$PROJECT_DIR/assets/icon.icns" "$RES_DIR/icon.icns"
    /usr/libexec/PlistBuddy \
        -c "Add :CFBundleIconFile string icon" \
        "$CONTENTS_DIR/Info.plist"
fi

if [[ "$NO_SIGN" != "1" ]]; then
    echo "==> Codesigning (identity: $SIGN_IDENTITY)"
    codesign --deep --force --sign "$SIGN_IDENTITY" "$APP_DIR"
fi

# Print a short summary with the bundle size so the caller can eyeball it.
bundle_size="$(du -sh "$APP_DIR" | awk '{print $1}')"
arch_info="$(lipo -archs "$MACOS_DIR/$BINARY_NAME" 2>/dev/null || file -b "$MACOS_DIR/$BINARY_NAME")"

echo ""
echo "Built $APP_DIR"
echo "  size: $bundle_size"
echo "  arch: $arch_info"
echo ""
echo "Launch with:"
echo "  open \"$APP_DIR\""
echo ""
echo "Note: the Sikarugir Wine runtime (~hundreds of MB) is downloaded on"
echo "first game launch into ~/Library/Application Support/${BUNDLE_ID}/,"
echo "not bundled inside the .app."
