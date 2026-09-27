#!/bin/bash
set -e

# Swift Parakeet Sidecar Build Script
# Builds the Swift sidecar binary for Voicetypr following Tauri v2 sidecar conventions

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$SCRIPT_DIR/dist"

echo "🔨 Building Swift Parakeet Sidecar..."

# Determine build configuration
BUILD_CONFIG="release"
CLEAN_BUILD=false

for arg in "$@"; do
    case "$arg" in
        --clean)
            CLEAN_BUILD=true
            ;;
        debug|release)
            BUILD_CONFIG="$arg"
            ;;
        *)
            echo "❌ Unknown argument: $arg"
            echo "Usage: $0 [debug|release] [--clean]"
            exit 1
            ;;
    esac
done

echo "📦 Build configuration: $BUILD_CONFIG"

# Keep SwiftPM build products by default so local builds are incremental.
if [ "$CLEAN_BUILD" = true ]; then
    echo "🧹 Cleaning SwiftPM build products..."
    rm -rf "$SCRIPT_DIR/.build"
fi

# Keep Swift scratch space on this volume instead of the system disk.
export TMPDIR="$SCRIPT_DIR/.tmp"
mkdir -p "$TMPDIR"

# Build Swift package
echo "🏗️  Compiling Swift package..."
cd "$SCRIPT_DIR"
swift build -c "$BUILD_CONFIG"

# Create dist directory
mkdir -p "$DIST_DIR"

# Determine Rust target triple (Tauri expects this format)
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    ARCH="aarch64"
fi

# Map to Rust target triple
case "$(uname -s)" in
    Darwin)
        TARGET_TRIPLE="${ARCH}-apple-darwin"
        ;;
    Linux)
        TARGET_TRIPLE="${ARCH}-unknown-linux-gnu"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        TARGET_TRIPLE="${ARCH}-pc-windows-msvc"
        ;;
    *)
        echo "❌ Unsupported platform: $(uname -s)"
        exit 1
        ;;
esac

echo "🖥️  Target triple: $TARGET_TRIPLE"

# Copy binary with correct name for Tauri
# Ask the active SwiftPM build system where it wrote this configuration.
# Swift Build can leave older .build/debug or .build/release products in place.
BIN_DIR=$(swift build -c "$BUILD_CONFIG" --show-bin-path)
BUILD_PATH="$BIN_DIR/ParakeetSidecar"

if [ ! -f "$BUILD_PATH" ]; then
    echo "❌ Error: Binary not found at $BUILD_PATH"
    exit 1
fi

# Copy with Tauri-expected naming: parakeet-sidecar-$TARGET_TRIPLE
OUTPUT_PATH="$DIST_DIR/parakeet-sidecar-$TARGET_TRIPLE"
cp "$BUILD_PATH" "$OUTPUT_PATH"
echo "✅ Binary copied to: $OUTPUT_PATH"

# Make executable
chmod +x "$OUTPUT_PATH"

# Create symlink for Tauri's externalBin configuration
# Tauri expects to find "parakeet-sidecar" during bundling, but at runtime
# it will automatically append the target triple and look for the full name
SYMLINK_PATH="$DIST_DIR/parakeet-sidecar"
ln -sf "$(basename "$OUTPUT_PATH")" "$SYMLINK_PATH"
echo "🔗 Symlink created: parakeet-sidecar -> $(basename "$OUTPUT_PATH")"

# Verify binary
echo "🔍 Verifying binary..."
if echo '{"type":"status"}' | "$OUTPUT_PATH" 2>/dev/null | grep -q '"type"'; then
    echo "✅ Binary verification successful!"
else
    echo "⚠️  Warning: Binary verification failed (this is OK if not on macOS 14+)"
fi

# Print size
SIZE=$(du -h "$OUTPUT_PATH" | cut -f1)
echo "📊 Binary size: $SIZE"

# List all binaries in dist
echo ""
echo "📁 Binaries in dist directory:"
ls -lh "$DIST_DIR"

echo ""
echo "🎉 Swift sidecar build complete!"
echo "   Binary: $OUTPUT_PATH"
echo "   Tauri will automatically resolve this when spawning 'parakeet-sidecar'"
