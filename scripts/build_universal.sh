#!/usr/bin/env bash
# Builds the macOS Universal 2 (Apple Silicon + Intel) app and DMG installers.
# Kept as an alias of build_installers.sh; pass --install to also install into /Applications.
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/build_installers.sh" "$@"
