#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALLERS_DIR="$ROOT_DIR/installers"
DESKTOP_DIR="$HOME/Desktop"

echo "=========================================================="
echo "  RevFly - Rebuild All Installer Packages"
echo "=========================================================="

# Ensure installers and desktop directories exist
mkdir -p "$INSTALLERS_DIR" "$DESKTOP_DIR"

# Detach any mounted RevFly volumes to avoid hdiutil locks
for vol in "/Volumes/RevFly"* "/Volumes/RevFly"*; do
  if [ -d "$vol" ]; then
    hdiutil detach "$vol" -force 2>/dev/null || true
  fi
done

# Check or download ONNX runtime for x86_64 if missing
ONNX_DIR="$ROOT_DIR/build_deps/onnxruntime-osx-x86_64-1.18.0/lib"
if [ ! -d "$ONNX_DIR" ]; then
  echo "--> Downloading ONNX Runtime for x86_64..."
  mkdir -p "$ROOT_DIR/build_deps"
  cd "$ROOT_DIR/build_deps"
  curl -L -O https://github.com/microsoft/onnxruntime/releases/download/v1.18.0/onnxruntime-osx-x86_64-1.18.0.tgz
  tar -xzf onnxruntime-osx-x86_64-1.18.0.tgz
fi

# 1. Build frontend
echo "--> 1. Building frontend assets..."
cd "$ROOT_DIR"
bun run build

# 2. Build arm64 release bundle (Apple Silicon)
echo "--> 2. Building Apple Silicon (arm64) release bundle..."
cd "$ROOT_DIR"
bun run tauri build

BASE_APP="$ROOT_DIR/src-tauri/target/release/bundle/macos/RevFly.app"
ARM64_BIN="$ROOT_DIR/src-tauri/target/release/revfly"

if [ ! -f "$ARM64_BIN" ] || [ ! -d "$BASE_APP" ]; then
  echo "Error: Apple Silicon build did not produce expected output at $BASE_APP"
  exit 1
fi

# 3. Build x86_64 release binary (Intel)
echo "--> 3. Building Intel (x86_64) release binary..."
cd "$ROOT_DIR/src-tauri"
MACOSX_DEPLOYMENT_TARGET=11.0 \
ORT_LIB_LOCATION="$ONNX_DIR" \
ORT_PREFER_DYNAMIC_LINK=1 \
cargo build --release --target x86_64-apple-darwin

X86_64_BIN="$ROOT_DIR/src-tauri/target/x86_64-apple-darwin/release/revfly"
if [ ! -f "$X86_64_BIN" ]; then
  echo "Error: Intel build did not produce expected output at $X86_64_BIN"
  exit 1
fi

sign_app_bundle() {
  local target="$1"
  if [ -d "$target" ]; then
    xattr -cr "$target" || true
    if [ -d "$target/Contents/Frameworks" ]; then
      for f in "$target/Contents/Frameworks"/*.dylib; do
        if [ -f "$f" ]; then
          codesign --force --sign - "$f" 2>/dev/null || true
        fi
      done
    fi
    codesign --force --deep --sign - \
      --identifier "com.revfly.desktop" \
      -r='designated => identifier "com.revfly.desktop"' \
      "$target" 2>/dev/null || true
  fi
}

make_dmg() {
  local app_source="$1"
  local dmg_name="$2"
  local vol_name="$3"

  echo "--> Packaging $dmg_name..."
  local stage_dir="/tmp/stage_dmg_$$"
  rm -rf "$stage_dir"
  mkdir -p "$stage_dir"
  cp -R "$app_source" "$stage_dir/"

  if [ ! -d "$stage_dir/RevFly.app" ]; then
    for d in "$stage_dir"/*; do
      if [ -d "$d" ] && [ "$(basename "$d")" != "Applications" ]; then
        mv "$d" "$stage_dir/RevFly.app"
        break
      fi
    done
  fi

  ln -s /Applications "$stage_dir/Applications"

  local out_installers="$INSTALLERS_DIR/$dmg_name"
  local out_desktop="$DESKTOP_DIR/$dmg_name"

  rm -f "$out_installers" "$out_desktop"
  hdiutil create -volname "$vol_name" -srcfolder "$stage_dir" -ov -format UDZO "$out_installers" > /dev/null
  rm -rf "$stage_dir"

  xattr -cr "$out_installers" || true
  codesign --force --sign - "$out_installers" 2>/dev/null || true

  # Copy to Desktop for immediate access
  cp "$out_installers" "$out_desktop"
  xattr -cr "$out_desktop" || true
  echo "✓ Created $dmg_name"
}

copy_resources() {
  local target_app="$1"
  mkdir -p "$target_app/Contents/Resources"
  if [ -f "$ROOT_DIR/src-tauri/icons/Assets.car" ]; then
    cp "$ROOT_DIR/src-tauri/icons/Assets.car" "$target_app/Contents/Resources/"
  fi
  if [ -f "$ROOT_DIR/src-tauri/icons/AppIcon.icns" ]; then
    cp "$ROOT_DIR/src-tauri/icons/AppIcon.icns" "$target_app/Contents/Resources/"
  fi
  if [ -f "$ROOT_DIR/src-tauri/icons/icon.icns" ]; then
    cp "$ROOT_DIR/src-tauri/icons/icon.icns" "$target_app/Contents/Resources/"
  fi
  if [ -f "$target_app/Contents/Info.plist" ]; then
    plutil -remove LSRequiresCarbon "$target_app/Contents/Info.plist" 2>/dev/null || true
    plutil -remove CFBundleIconName "$target_app/Contents/Info.plist" 2>/dev/null || true
    plutil -replace CFBundleIconFile -string "icon.icns" "$target_app/Contents/Info.plist" 2>/dev/null || true
  fi
  rm -f "$target_app/Contents/Resources/Assets.car"
}

# A. Universal Installer (arm64 + x86_64)
echo "--> 4. Creating Universal 2 (Intel + Apple Silicon) installer..."
UNIV_DIR="/tmp/aura_univ_$$"
UNIV_APP="$UNIV_DIR/RevFly.app"
rm -rf "$UNIV_DIR"
mkdir -p "$UNIV_DIR"
cp -R "$BASE_APP" "$UNIV_APP"
mkdir -p "$UNIV_APP/Contents/Frameworks"
if [ -d "$ONNX_DIR" ]; then
  cp "$ONNX_DIR"/libonnxruntime*.dylib "$UNIV_APP/Contents/Frameworks/"
fi
copy_resources "$UNIV_APP"
UNIV_BIN="/tmp/aura_univ_bin_$$"
lipo -create "$ARM64_BIN" "$X86_64_BIN" -output "$UNIV_BIN"
chmod +x "$UNIV_BIN"
install_name_tool -add_rpath "@executable_path/../Frameworks" "$UNIV_BIN" 2>/dev/null || true
cp "$UNIV_BIN" "$UNIV_APP/Contents/MacOS/revfly"
rm -f "$UNIV_BIN"
sign_app_bundle "$UNIV_APP"

# Update root app bundle
rm -rf "$ROOT_DIR/RevFly.app"
cp -R "$UNIV_APP" "$ROOT_DIR/RevFly.app"

# If --install passed, install to /Applications
if [ "$1" = "--install" ]; then
  echo "--> Installing to /Applications/RevFly.app..."
  rm -rf "/Applications/RevFly.app"
  cp -R "$UNIV_APP" "/Applications/RevFly.app"
  sign_app_bundle "/Applications/RevFly.app"
  touch "/Applications/RevFly.app"
  touch "/Applications/RevFly.app/Contents/Info.plist"
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f -r "/Applications/RevFly.app" 2>/dev/null || true
  rm -rf /private/var/folders/*/*/*/com.apple.iconservices* 2>/dev/null || true
  rm -rf /private/var/folders/*/*/*/com.apple.dock.iconcache* 2>/dev/null || true
  killall Finder Dock 2>/dev/null || true
  echo "✓ Installed to /Applications"
fi

make_dmg "$UNIV_APP" "RevFly_Universal.dmg" "RevFly Universal"
cp "$INSTALLERS_DIR/RevFly_Universal.dmg" "$INSTALLERS_DIR/RevFly_0.1.0_universal.dmg"
rm -rf "$UNIV_DIR"

# B. Intel Only Installer (x86_64)
echo "--> 5. Creating Intel (x86_64) installer..."
INTEL_DIR="/tmp/aura_intel_$$"
INTEL_APP="$INTEL_DIR/RevFly.app"
rm -rf "$INTEL_DIR"
mkdir -p "$INTEL_DIR"
cp -R "$BASE_APP" "$INTEL_APP"
mkdir -p "$INTEL_APP/Contents/Frameworks"
if [ -d "$ONNX_DIR" ]; then
  cp "$ONNX_DIR"/libonnxruntime*.dylib "$INTEL_APP/Contents/Frameworks/"
fi
cp "$X86_64_BIN" "$INTEL_APP/Contents/MacOS/revfly"
chmod +x "$INTEL_APP/Contents/MacOS/revfly"
install_name_tool -add_rpath "@executable_path/../Frameworks" "$INTEL_APP/Contents/MacOS/revfly" 2>/dev/null || true
copy_resources "$INTEL_APP"
sign_app_bundle "$INTEL_APP"
make_dmg "$INTEL_APP" "RevFly_Intel_x86_64.dmg" "RevFly Intel"
rm -rf "$INTEL_DIR"

# C. Apple Silicon Only Installer (arm64)
echo "--> 6. Creating Apple Silicon (arm64) installer..."
ARM_DIR="/tmp/aura_arm_$$"
ARM_APP="$ARM_DIR/RevFly.app"
rm -rf "$ARM_DIR"
mkdir -p "$ARM_DIR"
cp -R "$BASE_APP" "$ARM_APP"
rm -rf "$ARM_APP/Contents/Frameworks"
cp "$ARM64_BIN" "$ARM_APP/Contents/MacOS/revfly"
chmod +x "$ARM_APP/Contents/MacOS/revfly"
copy_resources "$ARM_APP"
sign_app_bundle "$ARM_APP"
make_dmg "$ARM_APP" "RevFly_Apple_Silicon_arm64.dmg" "RevFly Apple Silicon"
cp "$INSTALLERS_DIR/RevFly_Apple_Silicon_arm64.dmg" "$INSTALLERS_DIR/RevFly_0.1.0_aarch64.dmg"
rm -rf "$ARM_DIR"

# D. Package Windows & Linux Build Artifacts
echo "--> 7. Packaging Windows and Linux build archives..."
WIN_STAGE_NAME="aura_win_stage_$$"
WIN_STAGE="/tmp/$WIN_STAGE_NAME"
rm -rf "$WIN_STAGE"
mkdir -p "$WIN_STAGE"
cp "$ROOT_DIR/scripts/build_windows.bat" "$WIN_STAGE/" 2>/dev/null || true
cp "$ROOT_DIR/scripts/build_linux.sh" "$WIN_STAGE/" 2>/dev/null || true
mkdir -p "$WIN_STAGE/.github/workflows"
cp "$ROOT_DIR/.github/workflows/build-windows.yml" "$WIN_STAGE/.github/workflows/" 2>/dev/null || true
cp "$ROOT_DIR/.github/workflows/build-all-platforms.yml" "$WIN_STAGE/.github/workflows/" 2>/dev/null || true

cat << 'EOF' > "$WIN_STAGE/README_WINDOWS_AND_LINUX.txt"
============================================================
  RevFly - Windows & Linux Build Instructions
============================================================

--- WINDOWS ---
Option 1: Build on Windows PC
1. Copy this project to your Windows machine.
2. Install Node.js or Bun (https://bun.sh or https://nodejs.org).
3. Install Rust (https://rustup.rs).
4. Run build_windows.bat.
5. Installers (.exe and .msi) will be in:
   src-tauri\target\release\bundle\nsis\
   src-tauri\target\release\bundle\msi\

Option 2: Build with GitHub Actions
1. Push repository to GitHub.
2. Run workflow: "Build RevFly (All Platforms)" or "Build Windows Application".
3. Download artifact.

--- LINUX (Ubuntu / Debian / Fedora) ---
Option 1: Build on Linux PC
1. Open terminal in project root.
2. Run: bash scripts/build_linux.sh
3. Packages (.AppImage and .deb) will be in:
   src-tauri/target/release/bundle/appimage/
   src-tauri/target/release/bundle/deb/
============================================================
EOF

(cd /tmp && zip -rq "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$WIN_STAGE_NAME")
cp "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$DESKTOP_DIR/RevFly_Windows_Build.zip"
cp "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$INSTALLERS_DIR/RevFly_CrossPlatform_Build.zip"
cp "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$DESKTOP_DIR/RevFly_CrossPlatform_Build.zip"
rm -rf "$WIN_STAGE"

echo ""
echo "=========================================================="
echo "✓ All installer apps rebuilt successfully!"
echo "✓ Location: $INSTALLERS_DIR"
echo "   - RevFly_Universal.dmg (Intel + Apple Silicon)"
echo "   - RevFly_Apple_Silicon_arm64.dmg"
echo "   - RevFly_Intel_x86_64.dmg"
echo "   - RevFly_Windows_Build.zip"
echo "   - RevFly_CrossPlatform_Build.zip"
echo "✓ Also copied to: $DESKTOP_DIR"
echo "=========================================================="
