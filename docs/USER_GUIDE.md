# Micround User Guide

Complete guide to using Micround - the live microscope wallpaper application.

## Table of Contents

- [Overview](#overview)
- [System Tray Controls](#system-tray-controls)
- [Configuration](#configuration)
  - [Camera Settings](#camera-settings)
  - [Display Settings](#display-settings)
  - [Startup Options](#startup-options)
- [Features](#features)
  - [Live Preview](#live-preview)
  - [Pause and Freeze Frame](#pause-and-freeze-frame)
  - [Snapshots](#snapshots)
  - [Keyboard Shortcuts](#keyboard-shortcuts)
- [Multi-Monitor Setup](#multi-monitor-setup)
- [Performance Tips](#performance-tips)

## Overview

Micround displays a live feed from your USB microscope as your desktop wallpaper. The application runs in the background, using minimal system resources while providing a real-time view of your microscopic world.

### How It Works

1. **Capture** - Micround reads frames from your USB microscope camera
2. **Process** - Frames are scaled, rotated, and transformed as needed
3. **Display** - The processed image replaces your desktop wallpaper

The entire pipeline runs at 30 frames per second by default, ensuring smooth motion while keeping CPU usage low.

## System Tray Controls

After launching, Micround appears in your system tray (Windows/Linux) or menu bar (macOS).

### Icon States

| Icon | Meaning |
|------|---------|
| Colored microscope | Feed is running |
| Gray microscope | Feed is stopped |
| Microscope with pause | Feed is paused |

### Tray Menu

Right-click the tray icon for the menu:

- **Start Feed** / **Stop Feed** - Toggle the live wallpaper
- **Pause** / **Resume** - Freeze the current frame
- **Take Snapshot** - Save current frame to a file
- **Settings** - Open the settings window
- **Quit** - Exit Micround

## Configuration

Access settings by right-clicking the tray icon and selecting "Settings".

### Camera Settings

**Device Selection**
- Choose from detected cameras in the dropdown
- Click "Refresh" to rescan for cameras
- Microscope cameras often appear as "USB Camera" or similar

**Resolution**
- Higher resolution = more detail, more processing power
- Common options: 1920x1080, 1280x720, 640x480
- For most microscopes, 1280x720 offers a good balance

**Framerate**
- 30 fps - Smooth motion (default)
- 15 fps - Lower resource usage
- 60 fps - Very smooth (if camera supports it)

### Display Settings

**Target Display**
- Select which monitor shows the microscope wallpaper
- "Primary Display" is the default

**Scaling Mode**
- **Fit** - Scale to fit within the screen, maintaining aspect ratio (may show bars)
- **Fill** - Scale to fill the screen, maintaining aspect ratio (may crop edges)
- **Stretch** - Stretch to fill the screen (may distort image)
- **Center** - Display at original size in the center (may show borders)

**Rotation**
- Rotate the image: 0°, 90°, 180°, or 270°
- Useful if your microscope is mounted at an angle

**Flip**
- Horizontal flip - Mirror left-to-right
- Vertical flip - Mirror top-to-bottom
- Some microscope optics require flipping to show correct orientation

### Startup Options

**Launch at login**
- Automatically start Micround when you log in to your computer
- The feed doesn't start automatically unless combined with "Auto-start feed"

**Auto-start feed**
- Begin capturing and displaying immediately on launch
- Combine with "Launch at login" for a fully automatic experience

**Minimize to tray on startup**
- Hide the settings window when Micround starts
- The app will appear only in the system tray

## Features

### Live Preview

The Settings window includes a live preview showing:
- Current camera feed at reduced resolution
- Applied transforms (rotation, flip)
- Preview updates in real-time as you change settings

### Pause and Freeze Frame

**Pause** freezes the current frame on your wallpaper:
- Right-click tray icon → Pause
- Or use keyboard shortcut (default: Ctrl+Shift+P)

**Resume** returns to live feed:
- Right-click tray icon → Resume
- Or press the pause shortcut again

### Snapshots

Capture the current frame to a file:

1. Right-click tray icon → Take Snapshot
2. Or use keyboard shortcut (default: Ctrl+Shift+S)

Snapshots are saved to:
- **Windows:** `Pictures\Micround\`
- **macOS:** `~/Pictures/Micround/`
- **Linux:** `~/Pictures/Micround/`

File format: PNG with timestamp (e.g., `micround_2026-02-05_143022.png`)

### Keyboard Shortcuts

Default shortcuts (can be customized in Settings):

| Action | Shortcut |
|--------|----------|
| Toggle Feed | Ctrl+Shift+M |
| Pause/Resume | Ctrl+Shift+P |
| Take Snapshot | Ctrl+Shift+S |
| Open Settings | Ctrl+Shift+, |

## Multi-Monitor Setup

Micround supports multi-monitor configurations:

1. Open Settings
2. Under Display → Target, select the desired monitor
3. The microscope feed appears on the selected monitor only
4. Other monitors keep their normal wallpaper

**Tips:**
- Use a secondary monitor for the microscope so your primary stays usable
- Each monitor can have different scaling settings

## Performance Tips

### For Smooth Operation

- Close other applications using the camera
- Use 720p resolution instead of 1080p if needed
- Set framerate to 15fps for older systems
- Ensure your USB port provides sufficient power

### Resource Usage Targets

Normal operation should use approximately:
- **CPU:** Under 10%
- **Memory:** Under 200MB
- **GPU:** Under 15%

If usage is higher:
- Lower resolution
- Reduce framerate
- Check for other camera-using applications

### Thermal Considerations

Laptops may warm up during continuous operation. If this is an issue:
- Lower framerate to 15fps
- Use a laptop cooling pad
- Consider the "Pause" feature when not actively observing

---

For troubleshooting help, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
For answers to common questions, see [FAQ.md](FAQ.md).
