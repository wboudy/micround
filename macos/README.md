# macOS Packaging

This directory contains macOS-specific files for app bundle creation and distribution.

## Files

- `Info.plist` - Application metadata and permissions
- `Micround.entitlements` - Code signing entitlements for hardened runtime
- `com.micround.app.plist` - LaunchAgent for "Launch at login" feature

## Building

### Development Build (unsigned)

```bash
./scripts/build-macos.sh
```

Creates `target/release/Micround.app` without code signing.

### Release Build with DMG

```bash
./scripts/build-macos.sh --dmg
```

Creates both the app bundle and a DMG installer.

### Signed Release (for distribution)

```bash
export CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export NOTARIZE_PROFILE="your-notarization-profile"
./scripts/build-macos.sh --sign --dmg
```

This will:
1. Build the release binary
2. Create the app bundle
3. Sign with hardened runtime
4. Create DMG
5. Submit for notarization
6. Staple the notarization ticket

## Code Signing Setup

### 1. Get a Developer ID Certificate

You need an Apple Developer Program membership ($99/year) to get a Developer ID certificate for distribution outside the App Store.

### 2. Create Notarization Profile

Store your credentials in the keychain:

```bash
xcrun notarytool store-credentials "your-notarization-profile" \
    --apple-id "your@email.com" \
    --team-id "YOURTEAMID"
```

### 3. Environment Variables

- `CODESIGN_IDENTITY`: Your signing identity (e.g., "Developer ID Application: ...")
- `NOTARIZE_PROFILE`: Name of the stored keychain profile

## Launch at Login

The `com.micround.app.plist` file is a LaunchAgent template. The application handles installation/removal of this file when the user toggles "Launch at login" in settings.

Manual installation for testing:

```bash
cp macos/com.micround.app.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.micround.app.plist
```

## Requirements

- macOS 12.0 (Monterey) or later
- Xcode Command Line Tools
- Optional: `create-dmg` for prettier DMG (`brew install create-dmg`)
