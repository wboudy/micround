#!/bin/bash
# Build AppImage for Micround
#
# This script creates a self-contained AppImage that works across
# major Linux distributions without dependency issues.
#
# Requirements:
# - linuxdeploy (will be downloaded if not present)
# - appimagetool (included with linuxdeploy)
# - Built micround binary (cargo build --release)
#
# Usage:
#   ./scripts/build-appimage.sh
#
# Output:
#   dist/Micround-x86_64.AppImage

set -euo pipefail

# Configuration
APP_NAME="Micround"
APP_ID="io.github.micround"
BINARY_NAME="micround"
VERSION="${VERSION:-0.1.0}"

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/target/release"
DIST_DIR="$PROJECT_ROOT/dist"
APPDIR="$DIST_DIR/AppDir"

# Tool URLs
LINUXDEPLOY_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
LINUXDEPLOY="$DIST_DIR/linuxdeploy-x86_64.AppImage"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check for required tools
check_requirements() {
    log_info "Checking requirements..."

    if ! command -v cargo &> /dev/null; then
        log_error "cargo not found. Please install Rust."
        exit 1
    fi

    if ! command -v wget &> /dev/null && ! command -v curl &> /dev/null; then
        log_error "Neither wget nor curl found. Please install one of them."
        exit 1
    fi
}

# Download linuxdeploy if not present
download_linuxdeploy() {
    if [[ -f "$LINUXDEPLOY" ]]; then
        log_info "linuxdeploy already present"
        return
    fi

    log_info "Downloading linuxdeploy..."
    mkdir -p "$DIST_DIR"

    if command -v wget &> /dev/null; then
        wget -q -O "$LINUXDEPLOY" "$LINUXDEPLOY_URL"
    else
        curl -sL -o "$LINUXDEPLOY" "$LINUXDEPLOY_URL"
    fi

    chmod +x "$LINUXDEPLOY"
    log_info "linuxdeploy downloaded"
}

# Build the application
build_app() {
    log_info "Building Micround (release mode)..."
    cd "$PROJECT_ROOT"

    # Build with Linux feature
    cargo build --release --features linux

    if [[ ! -f "$BUILD_DIR/$BINARY_NAME" ]]; then
        log_error "Build failed: binary not found at $BUILD_DIR/$BINARY_NAME"
        exit 1
    fi

    log_info "Build complete"
}

# Create AppDir structure
create_appdir() {
    log_info "Creating AppDir structure..."

    # Clean previous build
    rm -rf "$APPDIR"
    mkdir -p "$APPDIR/usr/bin"
    mkdir -p "$APPDIR/usr/share/applications"
    mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
    mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"

    # Copy binary
    cp "$BUILD_DIR/$BINARY_NAME" "$APPDIR/usr/bin/"
    chmod +x "$APPDIR/usr/bin/$BINARY_NAME"

    log_info "AppDir structure created"
}

# Create desktop file
create_desktop_file() {
    log_info "Creating desktop file..."

    cat > "$APPDIR/usr/share/applications/$APP_ID.desktop" << EOF
[Desktop Entry]
Type=Application
Name=Micround
Comment=Live microscope camera feed as desktop wallpaper
Exec=micround
Icon=micround
Categories=Utility;Graphics;Video;
Terminal=false
StartupWMClass=micround
Keywords=microscope;camera;wallpaper;live;
EOF

    # Also create at AppDir root (required by AppImage)
    cp "$APPDIR/usr/share/applications/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"

    log_info "Desktop file created"
}

# Create icon
create_icon() {
    log_info "Creating application icon..."

    # For now, create a simple placeholder SVG icon
    # In production, this would be replaced with a proper icon
    cat > "$APPDIR/usr/share/icons/hicolor/scalable/apps/micround.svg" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#1a5276;stop-opacity:1" />
      <stop offset="100%" style="stop-color:#2874a6;stop-opacity:1" />
    </linearGradient>
  </defs>
  <!-- Background circle -->
  <circle cx="128" cy="128" r="120" fill="url(#bg)" stroke="#154360" stroke-width="4"/>
  <!-- Microscope lens representation -->
  <circle cx="128" cy="128" r="60" fill="none" stroke="#85c1e9" stroke-width="8"/>
  <circle cx="128" cy="128" r="40" fill="none" stroke="#aed6f1" stroke-width="4"/>
  <!-- Center dot (specimen) -->
  <circle cx="128" cy="128" r="12" fill="#58d68d"/>
  <!-- Cross hairs -->
  <line x1="128" y1="58" x2="128" y2="88" stroke="#d5dbdb" stroke-width="2"/>
  <line x1="128" y1="168" x2="128" y2="198" stroke="#d5dbdb" stroke-width="2"/>
  <line x1="58" y1="128" x2="88" y2="128" stroke="#d5dbdb" stroke-width="2"/>
  <line x1="168" y1="128" x2="198" y2="128" stroke="#d5dbdb" stroke-width="2"/>
</svg>
EOF

    # Create symlink at AppDir root (required by AppImage)
    ln -sf usr/share/icons/hicolor/scalable/apps/micround.svg "$APPDIR/micround.svg"

    log_info "Icon created"
}

# Create AppRun script
create_apprun() {
    log_info "Creating AppRun script..."

    cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/bash
# AppRun script for Micround AppImage

# Get the directory where this script is located
HERE="$(dirname "$(readlink -f "${0}")")"

# Set up environment
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS}"

# Run the application
exec "${HERE}/usr/bin/micround" "$@"
EOF

    chmod +x "$APPDIR/AppRun"
    log_info "AppRun script created"
}

# Bundle dependencies using linuxdeploy
bundle_dependencies() {
    log_info "Bundling dependencies with linuxdeploy..."

    # Run linuxdeploy to bundle dependencies
    # Note: This requires FUSE or --appimage-extract-and-run
    "$LINUXDEPLOY" \
        --appdir "$APPDIR" \
        --desktop-file "$APPDIR/usr/share/applications/$APP_ID.desktop" \
        --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/micround.svg" \
        --output appimage || {
            log_warn "linuxdeploy failed. Trying with --appimage-extract-and-run..."
            "$LINUXDEPLOY" --appimage-extract-and-run \
                --appdir "$APPDIR" \
                --desktop-file "$APPDIR/usr/share/applications/$APP_ID.desktop" \
                --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/micround.svg" \
                --output appimage
        }

    log_info "Dependencies bundled"
}

# Finalize AppImage
finalize() {
    log_info "Finalizing AppImage..."

    # Find the generated AppImage
    APPIMAGE_FILE=$(find "$DIST_DIR" -maxdepth 1 -name "*.AppImage" -type f 2>/dev/null | head -n1)

    if [[ -n "$APPIMAGE_FILE" ]]; then
        FINAL_NAME="$DIST_DIR/Micround-${VERSION}-x86_64.AppImage"
        mv "$APPIMAGE_FILE" "$FINAL_NAME"
        chmod +x "$FINAL_NAME"

        log_info "AppImage created: $FINAL_NAME"
        log_info "Size: $(du -h "$FINAL_NAME" | cut -f1)"
    else
        log_error "AppImage file not found"
        exit 1
    fi
}

# Clean up
cleanup() {
    log_info "Cleaning up..."
    rm -rf "$APPDIR"
    log_info "Cleanup complete"
}

# Main execution
main() {
    log_info "Building Micround AppImage v${VERSION}"
    log_info "================================"

    check_requirements
    download_linuxdeploy
    build_app
    create_appdir
    create_desktop_file
    create_icon
    create_apprun
    bundle_dependencies
    finalize
    cleanup

    log_info "================================"
    log_info "AppImage build complete!"
    log_info ""
    log_info "To run: ./dist/Micround-${VERSION}-x86_64.AppImage"
    log_info ""
    log_info "Note: User must be in 'video' group for camera access:"
    log_info "  sudo usermod -a -G video \$USER"
    log_info ""
}

main "$@"
