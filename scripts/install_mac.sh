#!/usr/bin/env bash
# RevFly - 1-Line Installer for macOS
# Automatically downloads, installs, and clears Gatekeeper quarantine flags.

set -e

REPO="${AURA_REPO:-eugeneotsevich/RevFly}"
APP_NAME="RevFly.app"
INSTALL_PATH="/Applications/$APP_NAME"
TEMP_DIR="/tmp/revfly_install_$$"

echo "=========================================================="
echo "  RevFly - macOS Quick Installer"
echo "=========================================================="

# Check if running on macOS
if [ "$(uname -s)" != "Darwin" ]; then
  echo "Error: This installer is only for macOS."
  exit 1
fi

# Detect CPU architecture
ARCH="$(uname -m)"
echo "--> Detected hardware: $ARCH"

mkdir -p "$TEMP_DIR"

cleanup() {
  if [ -n "$MOUNT_POINT" ] && [ -d "$MOUNT_POINT" ]; then
    hdiutil detach "$MOUNT_POINT" -force >/dev/null 2>&1 || true
  fi
  rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

# Check if local DMG exists in current directory or installers/
DMG_PATH=""
if [ -f "installers/RevFly_Universal.dmg" ]; then
  DMG_PATH="installers/RevFly_Universal.dmg"
elif [ -f "RevFly_Universal.dmg" ]; then
  DMG_PATH="RevFly_Universal.dmg"
elif [ -f "$HOME/Desktop/RevFly_Universal.dmg" ]; then
  DMG_PATH="$HOME/Desktop/RevFly_Universal.dmg"
fi

if [ -n "$DMG_PATH" ] && [ -f "$DMG_PATH" ]; then
  echo "--> Using local installer: $DMG_PATH"
  LOCAL_DMG="$DMG_PATH"
else
  # Download from GitHub Releases
  DMG_NAME="RevFly_Universal.dmg"
  DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$DMG_NAME"
  LOCAL_DMG="$TEMP_DIR/$DMG_NAME"
  
  echo "--> Downloading latest release from GitHub ($REPO)..."
  curl -fL --progress-bar "$DOWNLOAD_URL" -o "$LOCAL_DMG" || {
    echo ""
    echo "Warning: Direct release download not found at $DOWNLOAD_URL"
    echo "If you have not created a GitHub Release yet, run scripts/build_installers.sh first."
    exit 1
  }
fi

# Close existing running instance
echo "--> Closing any running instances of RevFly..."
pkill -f "revfly" >/dev/null 2>&1 || true

# Mount the disk image
echo "--> Mounting disk image..."
MOUNT_OUTPUT="$(hdiutil attach "$LOCAL_DMG" -nobrowse -readonly)"
MOUNT_POINT="$(echo "$MOUNT_OUTPUT" | grep "/Volumes/" | sed -E 's/.*(\/Volumes\/.*)/\1/' | head -n 1)"

if [ -z "$MOUNT_POINT" ] || [ ! -d "$MOUNT_POINT" ]; then
  echo "Error: Failed to mount disk image."
  exit 1
fi

# Check for app inside mounted DMG
SOURCE_APP="$MOUNT_POINT/$APP_NAME"
if [ ! -d "$SOURCE_APP" ]; then
  SOURCE_APP="$(find "$MOUNT_POINT" -maxdepth 2 -name "*.app" | head -n 1)"
fi

if [ -z "$SOURCE_APP" ] || [ ! -d "$SOURCE_APP" ]; then
  echo "Error: Application bundle not found inside disk image."
  exit 1
fi

# Copy app to /Applications
echo "--> Installing to /Applications..."
rm -rf "$INSTALL_PATH"
cp -R "$SOURCE_APP" "/Applications/"

# Unmount disk image
hdiutil detach "$MOUNT_POINT" -force >/dev/null 2>&1 || true
MOUNT_POINT=""

# Clear macOS quarantine flag (bypasses Gatekeeper "unidentified developer" block)
echo "--> Clearing macOS quarantine flag..."
xattr -cr "$INSTALL_PATH" || true

# Register with macOS LaunchServices
echo "--> Registering application with system..."
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$INSTALL_PATH" >/dev/null 2>&1 || true

echo ""
echo "=========================================================="
echo "✓ RevFly installed successfully into /Applications!"
echo "✓ Gatekeeper quarantine cleared (no unknown developer warning)."
echo "=========================================================="
echo ""
echo "To start the app:"
echo "1. Open Finder -> Applications -> RevFly"
echo "2. Grant Microphone and Accessibility permissions when asked."
echo ""
