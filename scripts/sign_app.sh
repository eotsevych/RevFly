#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "==> Preparing and signing RevFly macOS Universal bundle..."

SRC_APP="$ROOT_DIR/src-tauri/target/release/bundle/macos/RevFly.app"
ROOT_APP="$ROOT_DIR/RevFly.app"
UNIVERSAL_DMG="$ROOT_DIR/RevFly_0.1.0_universal.dmg"
AARCH64_DMG="$ROOT_DIR/RevFly_0.1.0_aarch64.dmg"

ARM64_BIN="$ROOT_DIR/src-tauri/target/release/revfly"
X86_64_BIN="$ROOT_DIR/src-tauri/target/x86_64-apple-darwin/release/revfly"
ONNX_DIR="$ROOT_DIR/build_deps/onnxruntime-osx-x86_64-1.18.0/lib"

if [ -f "$ARM64_BIN" ] && [ -f "$X86_64_BIN" ]; then
  echo "--> Combining arm64 and x86_64 binaries into Universal Mach-O executable..."
  UNIV_BIN="/tmp/revfly_univ_$$"
  lipo -create "$ARM64_BIN" "$X86_64_BIN" -output "$UNIV_BIN"
  chmod +x "$UNIV_BIN"
  
  # Ensure @executable_path/../Frameworks is in the rpath so x86_64 dyld can find libonnxruntime
  install_name_tool -add_rpath "@executable_path/../Frameworks" "$UNIV_BIN" 2>/dev/null || true
  
  # Install into SRC_APP
  cp "$UNIV_BIN" "$SRC_APP/Contents/MacOS/revfly"
  rm -f "$UNIV_BIN"
  
  # Copy Frameworks
  mkdir -p "$SRC_APP/Contents/Frameworks"
  if [ -d "$ONNX_DIR" ]; then
    cp "$ONNX_DIR"/libonnxruntime*.dylib "$SRC_APP/Contents/Frameworks/"
  fi
  echo "✓ Universal binary and Frameworks installed into bundle."
fi

sign_app() {
  local target="$1"
  if [ -d "$target" ]; then
    echo "Signing: $target"
    xattr -cr "$target" || true
    
    # Sign any frameworks or dylibs first
    if [ -d "$target/Contents/Frameworks" ]; then
      for f in "$target/Contents/Frameworks"/*.dylib; do
        if [ -f "$f" ]; then
          codesign --force --sign - "$f"
        fi
      done
    fi
    
    codesign --force --deep --sign - \
      --identifier "com.revfly.desktop" \
      -r='designated => identifier "com.revfly.desktop"' \
      "$target"
    codesign -dvvv "$target" 2>&1 | grep "Identifier="
    codesign -d -r- "$target" 2>&1 | grep "designated =>"
    echo "✓ Signed: $target"
  fi
}

sign_app "$SRC_APP"

# Synchronize root RevFly.app
if [ -d "$SRC_APP" ]; then
  echo "Updating root RevFly.app..."
  rm -rf "$ROOT_APP"
  cp -R "$SRC_APP" "$ROOT_APP"
  sign_app "$ROOT_APP"
fi

# Package fresh signed Universal DMG
if [ -d "$ROOT_APP" ]; then
  echo "Packaging signed Universal DMG..."
  STAGE_DIR="/tmp/aura_dmg_pack_$$"
  rm -rf "$STAGE_DIR"
  mkdir -p "$STAGE_DIR"
  cp -R "$ROOT_APP" "$STAGE_DIR/"
  ln -s /Applications "$STAGE_DIR/Applications"
  
  rm -f "$UNIVERSAL_DMG" "$AARCH64_DMG"
  hdiutil create -volname "RevFly" -srcfolder "$STAGE_DIR" -ov -format UDZO "$UNIVERSAL_DMG" > /dev/null
  rm -rf "$STAGE_DIR"
  xattr -cr "$UNIVERSAL_DMG" || true
  codesign --force --sign - "$UNIVERSAL_DMG" 2>/dev/null || true
  
  # Also copy to AARCH64_DMG for backwards compatibility
  cp "$UNIVERSAL_DMG" "$AARCH64_DMG"
  echo "✓ Signed Universal DMG ready at: $UNIVERSAL_DMG"
  echo "✓ Signed DMG ready at: $AARCH64_DMG"
fi

# Clean old installation from /Applications to allow clean install from scratch
echo "Cleaning old /Applications/RevFly.app..."
pkill -f "revfly" 2>/dev/null || true
rm -rf "/Applications/RevFly.app"
tccutil reset Accessibility com.revfly.desktop 2>/dev/null || true
tccutil reset Microphone com.revfly.desktop 2>/dev/null || true
tccutil reset All com.revfly.desktop 2>/dev/null || true

echo "==> All bundles signed and verified successfully."
