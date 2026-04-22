#!/usr/bin/env bash
# Regenerate assets/icon.icns and assets/icon.ico from
# assets/icon-source.png by running the icon-gen helper crate.
#
# Usage:
#   scripts/generate-icons.sh
#
# Drop a new source PNG into assets/icon-source.png and re-run.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../icon-gen"
exec cargo run --release
