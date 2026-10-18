#!/usr/bin/env bash
# Builds revfly-translate, the helper for the "Apple" translation provider, as a universal
# (arm64 + x86_64) binary. Run by src-tauri/build.rs on macOS; can also be run by hand:
#   scripts/build_apple_translate.sh [output-path]
#
# Needs Xcode (or the Command Line Tools) with the macOS 26 SDK or newer: the helper uses
# TranslationSession(installedSource:target:), which is macOS 26+. The helper's own deployment
# target is 26.0; RevFly itself still runs on macOS 11 and only starts the helper on macOS 26+.
#
# swiftc is used directly rather than SwiftPM: SwiftPM records the deployment target as the SDK
# version, and the Translation framework picks its default model from that version.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC="$ROOT_DIR/src-tauri/apple-translate/revfly-translate.swift"
OUT="${1:-$ROOT_DIR/src-tauri/binaries/revfly-translate-universal-apple-darwin}"
MIN_MACOS="26.0"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/revfly-translate.XXXXXX")"
trap 'rm -r "$WORK"' EXIT

SDK="$(xcrun --sdk macosx --show-sdk-path)"
for arch in arm64 x86_64; do
  xcrun --sdk macosx swiftc -O -whole-module-optimization \
    -sdk "$SDK" -target "$arch-apple-macos$MIN_MACOS" \
    "$SRC" -o "$WORK/revfly-translate-$arch"
done

mkdir -p "$(dirname "$OUT")"
lipo -create "$WORK/revfly-translate-arm64" "$WORK/revfly-translate-x86_64" -output "$WORK/revfly-translate"
# Signed again with the app bundle; ad-hoc here so the binary runs straight from the build.
codesign --force --sign - --options runtime "$WORK/revfly-translate"
mv "$WORK/revfly-translate" "$OUT"
echo "Built $OUT ($(lipo -archs "$OUT"))"
