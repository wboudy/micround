# Getting Started with Micround

Transform your desktop wallpaper into a live microscope feed in minutes.

## What You'll Need

- A USB microscope or compatible webcam
- Windows 10/11, macOS 12+, or Linux with X11
- About 5 minutes

## Quick Start

### 1. Download Micround

| Platform | Download |
|----------|----------|
| Windows | [Micround-0.1.0.msi](releases) |
| macOS | [Micround-0.1.0.dmg](releases) |
| Linux | [Micround-0.1.0.AppImage](releases) |

### 2. Install

**Windows:**
1. Run the downloaded `.msi` file
2. Follow the installation wizard
3. Launch from Start menu or desktop shortcut

**macOS:**
1. Open the downloaded `.dmg` file
2. Drag Micround to your Applications folder
3. Launch from Applications (you may need to right-click → Open the first time)
4. Allow camera access when prompted

**Linux:**
1. Make the AppImage executable: `chmod +x Micround-0.1.0.AppImage`
2. Run: `./Micround-0.1.0.AppImage`
3. Grant camera permissions if prompted

### 3. Connect Your Microscope

1. Plug in your USB microscope
2. Wait for your system to recognize it (usually 5-10 seconds)

### 4. First Run

When you launch Micround for the first time:

1. **Welcome screen** - Click "Get Started"
2. **Camera detection** - Micround scans for connected cameras
3. **Select your microscope** - Choose from the list of detected cameras
4. **Permission check** (macOS) - Grant camera access if prompted
5. **Ready!** - Your microscope feed appears on your desktop

### 5. Basic Controls

Once running, Micround lives in your system tray (Windows/Linux) or menu bar (macOS):

- **Left-click** the icon to toggle the feed on/off
- **Right-click** for the menu:
  - Start/Stop Feed
  - Pause (freeze current frame)
  - Take Snapshot
  - Settings
  - Quit

## What's Next?

- [Configure camera and display settings](USER_GUIDE.md#configuration)
- [Adjust scaling and rotation](USER_GUIDE.md#display-settings)
- [Set up launch at login](USER_GUIDE.md#startup-options)
- [Troubleshooting common issues](TROUBLESHOOTING.md)

## Quick Troubleshooting

**Camera not detected?**
- Unplug and replug the USB cable
- Try a different USB port
- Check if another app is using the camera

**Feed not showing as wallpaper?**
- Some desktop environments have restrictions
- On Linux, make sure you're running X11 (not Wayland)
- Try restarting Micround

**Performance issues?**
- Lower the resolution in Settings
- Reduce framerate to 15fps
- Close other camera-using applications

For more help, see the [Troubleshooting Guide](TROUBLESHOOTING.md).
