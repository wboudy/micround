# Micround Build Scripts

This directory contains build and packaging scripts for Micround.

## Scripts

### ci-loop.sh

CI feedback loop automation for rapid development iteration.

**Requirements:**
- gh (GitHub CLI) installed and authenticated
- jq (JSON processor)
- git

**Usage:**
```bash
./scripts/ci-loop.sh watch         # Poll CI until completion
./scripts/ci-loop.sh logs [RUN_ID] # Fetch failed job logs
./scripts/ci-loop.sh full          # Push + watch + logs on failure
./scripts/ci-loop.sh status        # Show latest run info
./scripts/ci-loop.sh help          # Show usage
```

**Environment variables:**
- `POLL_INTERVAL`: Seconds between CI status checks (default: 30)
- `MAX_WAIT`: Max seconds before timeout (default: 600)

**Exit codes:**
- 0: CI passed / success
- 1: CI failed / error
- 2: Timeout waiting for CI

**Example workflow:**
```bash
# Make changes, commit, then run full CI loop
git commit -am "Fix bug"
./scripts/ci-loop.sh full
# Script will push, watch CI, and show logs on failure
```

### build-appimage.sh

Creates a self-contained AppImage for Linux distribution.

**Requirements:**
- Rust toolchain (cargo)
- wget or curl (for downloading linuxdeploy)
- FUSE (for running AppImage, or use --appimage-extract-and-run)

**Usage:**
```bash
./scripts/build-appimage.sh
```

**Output:**
- `dist/Micround-<version>-x86_64.AppImage`

**What it does:**
1. Builds Micround in release mode with Linux features
2. Downloads linuxdeploy (if not present)
3. Creates AppDir structure
4. Bundles dependencies
5. Creates final AppImage

**Environment variables:**
- `VERSION`: Set the version string (default: 0.1.0)

Example:
```bash
VERSION=1.0.0 ./scripts/build-appimage.sh
```

## Camera Access

On Linux, users need camera access permissions. The most common approach is adding the user to the 'video' group:

```bash
sudo usermod -a -G video $USER
# Log out and back in for changes to take effect
```

Alternatively, a udev rule can grant access to specific devices.

## Testing AppImages

To test an AppImage without FUSE:
```bash
./Micround-x86_64.AppImage --appimage-extract
./squashfs-root/AppRun
```

## Cross-Distribution Testing

Test the AppImage on:
- Ubuntu 20.04+ LTS
- Fedora 36+
- Arch Linux
- Debian 11+
- openSUSE Tumbleweed
