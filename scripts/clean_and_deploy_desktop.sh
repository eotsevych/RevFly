#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DESKTOP_DIR="$HOME/Desktop"
INSTALLERS_DIR="$ROOT_DIR/installers"
mkdir -p "$DESKTOP_DIR" "$INSTALLERS_DIR"

echo "=========================================================="
echo "  RevFly - Clean Rebuild & Desktop Deploy Script"
echo "=========================================================="

# 1. Stop running instances
echo "--> 1. Stopping any running RevFly instances..."
pkill -9 -f "revfly" 2>/dev/null || true
pkill -9 -f "RevFly" 2>/dev/null || true
sleep 1

# 2. Delete old installed app
echo "--> 2. Deleting old app from /Applications..."
rm -rf "/Applications/RevFly.app"
echo "✓ Deleted /Applications/RevFly.app"

# 3. Reset macOS permissions (Accessibility & Microphone)
echo "--> 3. Resetting macOS permissions (Accessibility & Microphone)..."
tccutil reset Accessibility com.revfly.desktop 2>/dev/null || true
tccutil reset Microphone com.revfly.desktop 2>/dev/null || true
tccutil reset All com.revfly.desktop 2>/dev/null || true
echo "✓ Permissions reset"

# 4. Clear application data, caches, and preferences for clean install
echo "--> 4. Clearing previous app settings and databases..."
rm -rf "$HOME/Library/Application Support/revfly"
rm -rf "$HOME/Library/Application Support/com.revfly.desktop"
rm -rf "$HOME/Library/Caches/com.revfly.desktop"
rm -rf "$HOME/Library/WebKit/com.revfly.desktop"
rm -rf "$HOME/Library/Preferences/com.revfly.desktop.plist"
rm -rf "$HOME/Library/Saved Application State/com.revfly.desktop.savedState"
echo "✓ App data cleared"

# 5. Ensure binaries are built
BASE_APP="$ROOT_DIR/src-tauri/target/release/bundle/macos/RevFly.app"
ARM64_BIN="$ROOT_DIR/src-tauri/target/release/revfly"
X86_64_BIN="$ROOT_DIR/src-tauri/target/x86_64-apple-darwin/release/revfly"
ONNX_DIR="$ROOT_DIR/build_deps/onnxruntime-osx-x86_64-1.18.0/lib"

if [ ! -f "$ARM64_BIN" ] || [ ! -d "$BASE_APP" ] || [ "${FORCE_REBUILD:-0}" = "1" ]; then
  echo "--> Building arm64 release bundle..."
  cd "$ROOT_DIR"
  bun run build
  bun run tauri build
fi

if [ ! -f "$X86_64_BIN" ] || [ "${FORCE_REBUILD:-0}" = "1" ]; then
  echo "--> Building x86_64 release binary..."
  cd "$ROOT_DIR/src-tauri"
  MACOSX_DEPLOYMENT_TARGET=11.0 \
  ORT_LIB_LOCATION="$ONNX_DIR" \
  ORT_PREFER_DYNAMIC_LINK=1 \
  cargo build --release --target x86_64-apple-darwin
fi

# Prepare target directory on Desktop
mkdir -p "$INSTALLERS_DIR"

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
  
  # Ensure the application bundle is named RevFly.app
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
  
  # Copy directly to Desktop as well
  cp "$out_installers" "$out_desktop"
  xattr -cr "$out_desktop" || true
  echo "✓ Created $dmg_name on Desktop"
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

# A. Package Universal Installer (arm64 + x86_64)
echo "--> Creating Universal 2 (Intel + Apple Silicon) bundle..."
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

# Update root RevFly.app with Universal bundle
rm -rf "$ROOT_DIR/RevFly.app"
cp -R "$UNIV_APP" "$ROOT_DIR/RevFly.app"

# Install directly to /Applications
rm -rf "/Applications/RevFly.app"
cp -R "$UNIV_APP" "/Applications/RevFly.app"
sign_app_bundle "/Applications/RevFly.app"

# Register with LaunchServices and refresh icon cache
touch "/Applications/RevFly.app"
touch "/Applications/RevFly.app/Contents/Info.plist"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f -r "/Applications/RevFly.app" 2>/dev/null || true
rm -rf /private/var/folders/*/*/*/com.apple.iconservices* 2>/dev/null || true
rm -rf /private/var/folders/*/*/*/com.apple.dock.iconcache* 2>/dev/null || true
killall Finder Dock 2>/dev/null || true

make_dmg "$UNIV_APP" "RevFly_Universal.dmg" "RevFly Universal"

# Synchronize root universal DMG
cp "$INSTALLERS_DIR/RevFly_Universal.dmg" "$ROOT_DIR/RevFly_0.1.0_universal.dmg"

rm -rf "$UNIV_DIR"

# B. Package Intel Only Installer (x86_64)
echo "--> Creating Intel Only (x86_64) bundle..."
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

# C. Package Apple Silicon Only Installer (arm64)
echo "--> Creating Apple Silicon Only (arm64) bundle..."
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

# Synchronize root aarch64 DMG
cp "$INSTALLERS_DIR/RevFly_Apple_Silicon_arm64.dmg" "$ROOT_DIR/RevFly_0.1.0_aarch64.dmg"

rm -rf "$ARM_DIR"

# D. Package Windows & Linux Build Tools & Instructions
echo "--> Packaging Windows and Linux build package..."
WIN_STAGE_NAME="aura_win_stage_$$"
WIN_STAGE="/tmp/$WIN_STAGE_NAME"
rm -rf "$WIN_STAGE"
mkdir -p "$WIN_STAGE"
cp "$ROOT_DIR/scripts/build_windows.bat" "$WIN_STAGE/"
cp "$ROOT_DIR/scripts/build_linux.sh" "$WIN_STAGE/"
mkdir -p "$WIN_STAGE/.github/workflows"
cp "$ROOT_DIR/.github/workflows/build-windows.yml" "$WIN_STAGE/.github/workflows/" 2>/dev/null || true
cp "$ROOT_DIR/.github/workflows/build-all-platforms.yml" "$WIN_STAGE/.github/workflows/" 2>/dev/null || true

cat << 'EOF' > "$WIN_STAGE/README_WINDOWS_AND_LINUX.txt"
============================================================
  RevFly - Windows & Linux Build Instructions
============================================================

--- WINDOWS ---
Option 1: Build on any Windows PC
1. Copy this project to your Windows machine.
2. Install Node.js or Bun (https://bun.sh or https://nodejs.org).
3. Install Rust (https://rustup.rs).
4. Double-click "build_windows.bat".
5. Output installers (.exe and .msi) will be generated in:
   src-tauri\target\release\bundle\nsis\
   src-tauri\target\release\bundle\msi\

Option 2: Build automatically with GitHub Actions
1. Push your repository to GitHub.
2. Go to Actions -> "Build RevFly (All Platforms)" or "Build Windows Application".
3. Click "Run workflow".
4. Download the generated "revfly-windows-installer" artifact containing the Windows .exe installer.

--- LINUX (Ubuntu / Debian / Fedora) ---
Option 1: Build on your Linux machine
1. Open a terminal in the project directory.
2. Run: bash scripts/build_linux.sh
3. Output .AppImage and .deb packages will be generated in:
   src-tauri/target/release/bundle/appimage/
   src-tauri/target/release/bundle/deb/

Option 2: Build automatically with GitHub Actions
1. Push your repository to GitHub.
2. Go to Actions -> "Build RevFly (All Platforms)".
3. Click "Run workflow" -> download "revfly-linux-installer" artifact.
============================================================
EOF

(cd /tmp && zip -rq "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$WIN_STAGE_NAME")
cp "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$DESKTOP_DIR/RevFly_Windows_Build.zip"
cp "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$INSTALLERS_DIR/RevFly_CrossPlatform_Build.zip"
cp "$INSTALLERS_DIR/RevFly_Windows_Build.zip" "$DESKTOP_DIR/RevFly_CrossPlatform_Build.zip"
rm -rf "$WIN_STAGE"

echo ""
echo "=========================================================="
echo "✓ All done! Old app and permissions removed."
echo "✓ Fresh installers copied to Desktop:"
echo "   - $DESKTOP_DIR/RevFly_Universal.dmg"
echo "   - $DESKTOP_DIR/RevFly_Intel_x86_64.dmg"
echo "   - $DESKTOP_DIR/RevFly_Apple_Silicon_arm64.dmg"
echo "   - $DESKTOP_DIR/RevFly_Windows_Build.zip"
echo "   - $DESKTOP_DIR/RevFly_CrossPlatform_Build.zip"
echo "   - Folder: $INSTALLERS_DIR/"
echo "=========================================================="
