#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "==> Building RevFly Universal 2 (Apple Silicon + Intel)..."

# Detach any existing mounted volumes to prevent hdiutil conflicts
for vol in "/Volumes/RevFly"* "/Volumes/RevFly"*; do
  if [ -d "$vol" ]; then
    hdiutil detach "$vol" -force 2>/dev/null || true
  fi
done

# 1. Download and extract ONNX runtime for x86_64 (v1.18.0 supports macOS 11.0+) if not already present
mkdir -p "$ROOT_DIR/build_deps"
if [ ! -d "$ROOT_DIR/build_deps/onnxruntime-osx-x86_64-1.18.0/lib" ]; then
  echo "--> Downloading ONNX Runtime for macOS x86_64 (macOS 11.0+ compatible)..."
  cd "$ROOT_DIR/build_deps"
  curl -L -O https://github.com/microsoft/onnxruntime/releases/download/v1.18.0/onnxruntime-osx-x86_64-1.18.0.tgz
  tar -xzf onnxruntime-osx-x86_64-1.18.0.tgz
fi

# 2. Build frontend and aarch64 bundle
echo "--> Building arm64 release bundle..."
cd "$ROOT_DIR"
bun run build
bun run tauri build

# 3. Build x86_64 release binary (targeting macOS 11.0+)
echo "--> Building x86_64 release binary..."
cd "$ROOT_DIR/src-tauri"
MACOSX_DEPLOYMENT_TARGET=11.0 \
ORT_LIB_LOCATION="$ROOT_DIR/build_deps/onnxruntime-osx-x86_64-1.18.0/lib" \
ORT_PREFER_DYNAMIC_LINK=1 \
cargo build --release --target x86_64-apple-darwin

# 4. Package installers and deploy cleanly to Desktop
echo "--> Packaging and deploying to Desktop..."
"$ROOT_DIR/scripts/clean_and_deploy_desktop.sh"

echo "==> Build and Desktop deployment completed successfully."
