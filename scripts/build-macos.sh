#!/bin/bash
# Build macOS app bundle and DMG for Micround
#
# Usage:
#   ./scripts/build-macos.sh              # Build app bundle only
#   ./scripts/build-macos.sh --dmg        # Build app bundle and DMG
#   ./scripts/build-macos.sh --sign       # Build, sign, and notarize
#
# Environment variables:
#   CODESIGN_IDENTITY   - Code signing identity (e.g., "Developer ID Application: ...")
#   NOTARIZE_PROFILE    - Notarization keychain profile name

set -e

# Configuration
APP_NAME="Micround"
BUNDLE_ID="com.micround.app"
VERSION="0.1.0"
MIN_MACOS="12.0"

# Paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
MACOS_DIR="$PROJECT_DIR/macos"
BUILD_DIR="$PROJECT_DIR/target/release"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"
DMG_NAME="$APP_NAME-$VERSION.dmg"
DMG_PATH="$BUILD_DIR/$DMG_NAME"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Parse arguments
BUILD_DMG=false
SIGN_APP=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --dmg)
            BUILD_DMG=true
            shift
            ;;
        --sign)
            SIGN_APP=true
            shift
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Check prerequisites
check_prerequisites() {
    info "Checking prerequisites..."

    if ! command -v cargo &> /dev/null; then
        error "Rust/cargo not found. Install from https://rustup.rs"
    fi

    if ! command -v xcrun &> /dev/null; then
        error "Xcode command line tools not found. Run: xcode-select --install"
    fi

    if $BUILD_DMG && ! command -v create-dmg &> /dev/null; then
        warn "create-dmg not found. Install with: brew install create-dmg"
        warn "Will use basic hdiutil instead"
    fi

    if $SIGN_APP && [[ -z "$CODESIGN_IDENTITY" ]]; then
        error "CODESIGN_IDENTITY environment variable not set"
    fi
}

# Build the Rust binary
build_binary() {
    info "Building release binary..."
    cd "$PROJECT_DIR"

    # Build with macOS feature enabled
    cargo build --release --features macos

    if [[ ! -f "$BUILD_DIR/micround" ]]; then
        error "Build failed: binary not found"
    fi

    info "Binary built successfully"
}

# Create the app bundle structure
create_app_bundle() {
    info "Creating app bundle..."

    # Clean any existing bundle
    rm -rf "$APP_BUNDLE"

    # Create directory structure
    mkdir -p "$APP_BUNDLE/Contents/MacOS"
    mkdir -p "$APP_BUNDLE/Contents/Resources"
    mkdir -p "$APP_BUNDLE/Contents/Frameworks"

    # Copy binary
    cp "$BUILD_DIR/micround" "$APP_BUNDLE/Contents/MacOS/"

    # Copy Info.plist
    if [[ -f "$MACOS_DIR/Info.plist" ]]; then
        cp "$MACOS_DIR/Info.plist" "$APP_BUNDLE/Contents/"
    else
        error "Info.plist not found at $MACOS_DIR/Info.plist"
    fi

    # Copy icon if exists
    if [[ -f "$PROJECT_DIR/assets/AppIcon.icns" ]]; then
        cp "$PROJECT_DIR/assets/AppIcon.icns" "$APP_BUNDLE/Contents/Resources/"
    else
        warn "AppIcon.icns not found, app will have default icon"
    fi

    # Create PkgInfo
    echo -n "APPL????" > "$APP_BUNDLE/Contents/PkgInfo"

    info "App bundle created at $APP_BUNDLE"
}

# Sign the app bundle
sign_app_bundle() {
    if ! $SIGN_APP; then
        warn "Skipping code signing (use --sign to enable)"
        return
    fi

    info "Signing app bundle..."

    # Sign frameworks first (if any)
    if [[ -d "$APP_BUNDLE/Contents/Frameworks" ]]; then
        for framework in "$APP_BUNDLE/Contents/Frameworks/"*; do
            if [[ -d "$framework" ]]; then
                codesign --force --sign "$CODESIGN_IDENTITY" \
                    --options runtime \
                    --entitlements "$MACOS_DIR/Micround.entitlements" \
                    "$framework"
            fi
        done
    fi

    # Sign the main bundle
    codesign --force --sign "$CODESIGN_IDENTITY" \
        --options runtime \
        --entitlements "$MACOS_DIR/Micround.entitlements" \
        --deep \
        "$APP_BUNDLE"

    # Verify signature
    codesign --verify --verbose "$APP_BUNDLE"

    info "App bundle signed successfully"
}

# Create DMG
create_dmg() {
    if ! $BUILD_DMG; then
        return
    fi

    info "Creating DMG..."

    # Remove existing DMG
    rm -f "$DMG_PATH"

    if command -v create-dmg &> /dev/null; then
        # Use create-dmg for a nicer DMG
        create-dmg \
            --volname "$APP_NAME" \
            --volicon "$PROJECT_DIR/assets/AppIcon.icns" \
            --window-pos 200 120 \
            --window-size 600 400 \
            --icon-size 100 \
            --icon "$APP_NAME.app" 150 190 \
            --hide-extension "$APP_NAME.app" \
            --app-drop-link 450 190 \
            --no-internet-enable \
            "$DMG_PATH" \
            "$APP_BUNDLE" || {
                # Fallback to basic DMG if create-dmg fails
                warn "create-dmg failed, using basic hdiutil"
                create_basic_dmg
            }
    else
        create_basic_dmg
    fi

    info "DMG created at $DMG_PATH"
}

# Create basic DMG using hdiutil
create_basic_dmg() {
    local temp_dir=$(mktemp -d)
    local dmg_temp="$temp_dir/temp.dmg"

    # Copy app to temp directory
    cp -r "$APP_BUNDLE" "$temp_dir/"

    # Create symlink to Applications
    ln -s /Applications "$temp_dir/Applications"

    # Create DMG
    hdiutil create -volname "$APP_NAME" \
        -srcfolder "$temp_dir" \
        -ov -format UDZO \
        "$DMG_PATH"

    # Cleanup
    rm -rf "$temp_dir"
}

# Notarize the app
notarize_app() {
    if ! $SIGN_APP; then
        return
    fi

    if [[ -z "$NOTARIZE_PROFILE" ]]; then
        warn "NOTARIZE_PROFILE not set, skipping notarization"
        return
    fi

    info "Submitting for notarization..."

    local target_file
    if $BUILD_DMG; then
        target_file="$DMG_PATH"
    else
        # Need to create a zip for notarization of .app
        local zip_path="$BUILD_DIR/$APP_NAME.zip"
        ditto -c -k --keepParent "$APP_BUNDLE" "$zip_path"
        target_file="$zip_path"
    fi

    # Submit for notarization
    xcrun notarytool submit "$target_file" \
        --keychain-profile "$NOTARIZE_PROFILE" \
        --wait

    # Staple the result
    if $BUILD_DMG; then
        xcrun stapler staple "$DMG_PATH"
    else
        xcrun stapler staple "$APP_BUNDLE"
    fi

    info "Notarization complete"
}

# Main build process
main() {
    info "Building $APP_NAME v$VERSION for macOS"
    info "Options: DMG=$BUILD_DMG, Sign=$SIGN_APP"

    check_prerequisites
    build_binary
    create_app_bundle
    sign_app_bundle
    create_dmg
    notarize_app

    echo ""
    info "Build complete!"
    info "App bundle: $APP_BUNDLE"
    if $BUILD_DMG; then
        info "DMG: $DMG_PATH"
    fi

    if ! $SIGN_APP; then
        echo ""
        warn "App is not signed. To distribute:"
        warn "  1. Set CODESIGN_IDENTITY to your Developer ID"
        warn "  2. Set NOTARIZE_PROFILE to your notarization profile"
        warn "  3. Run: ./scripts/build-macos.sh --sign --dmg"
    fi
}

main "$@"
