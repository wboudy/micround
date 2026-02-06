# Frequently Asked Questions

Common questions about Micround.

## General

### What is Micround?

Micround is a desktop application that displays a live feed from your USB microscope as your desktop wallpaper. It's designed for scientists, hobbyists, educators, and anyone who wants a dynamic, scientific-looking desktop.

### What cameras are supported?

Micround works with most USB microscopes and webcams that support standard video protocols:

**Supported:**
- Most USB microscopes (generic Chinese microscopes, AmScope, Celestron, etc.)
- UVC-compliant webcams
- Cameras that work with standard system camera apps

**Not supported:**
- Wireless/WiFi cameras
- IP cameras
- HDMI capture devices (without additional software)
- Proprietary cameras requiring special drivers

### Does Micround record video?

No, Micround is designed for live display only. It does not record video.

You can:
- Take snapshots (still images) at any time
- Use the "Pause" feature to freeze a frame

For video recording, use dedicated microscope software alongside Micround.

### Can I use it with multiple monitors?

Yes! You can:
- Display the microscope feed on one monitor
- Keep your normal wallpaper on other monitors
- Choose which monitor shows the feed in Settings

### Does it work without a microscope?

Yes, Micround works with any webcam. You could use it to display:
- A nature cam pointed at a bird feeder
- A time-lapse view out your window
- Any live camera feed

The term "microscope" is just the primary use case.

## Privacy

### Does Micround send my camera feed anywhere?

**No.** Micround is completely local and offline:
- No network connections
- No cloud services
- No analytics or telemetry
- Your camera feed never leaves your computer

See our [Privacy Policy](PRIVACY.md) for details.

### Does Micround work when my computer is locked?

No. For privacy and security reasons, Micround stops the camera feed when:
- The screen is locked
- The user session is switched
- The system goes to sleep

The feed automatically resumes when you unlock.

## Performance

### How much CPU does Micround use?

On modern hardware with GPU acceleration:
- **Idle desktop:** Under 10% CPU
- **Active use:** 5-15% CPU depending on resolution

Without GPU acceleration, CPU usage may be higher.

### How much memory does Micround use?

Typically under 200MB RAM, depending on resolution:
- 480p: ~80MB
- 720p: ~120MB
- 1080p: ~180MB

### Will Micround make my laptop hot?

Possibly, during extended use. To minimize heat:
- Lower resolution to 720p
- Reduce framerate to 15fps
- Use the "Pause" feature when not actively observing
- Consider a laptop cooling pad

### Does Micround work on battery?

Yes, but it will use battery power. For longer battery life:
- Lower framerate to 15fps
- Use lower resolution
- Pause the feed when not needed

## Compatibility

### Does Micround work with Wayland (Linux)?

Not currently. Micround requires X11 for wallpaper manipulation on Linux.

If you're using a Wayland compositor:
- Switch to an X11 session at login
- Or use Xwayland (limited support)

Future versions may add native Wayland support.

### Does Micround work with virtual desktops?

Yes, with some considerations:
- The feed shows on the physical display
- Virtual desktop behavior varies by OS:
  - **Windows:** Feed shows on all virtual desktops
  - **macOS:** Feed shows on current space
  - **Linux:** Depends on compositor

### Can I use Micround alongside other wallpaper apps?

Generally, no. Only one application can control the wallpaper at a time.

If you have another wallpaper app (Wallpaper Engine, Lively, etc.), you may need to:
- Close the other app before starting Micround
- Or configure the other app to exclude the monitor you want for Micround

### Does Micround work with dark mode?

Yes! Micround doesn't have a visible window during normal operation - it just sets your wallpaper. The settings window follows your system theme.

## Features

### Can I add text or timestamps to the image?

Not in version 0.1.0. Overlay features may be added in future versions.

### Can I crop the microscope image?

Not directly. However, you can:
- Use the "Fill" scaling mode to zoom and crop edges
- Use the "Center" mode to show original size with borders

### Can I save my settings profiles?

Not currently. Settings are global for the application.

### Does Micround support hotkeys?

Yes! Default keyboard shortcuts:
- Ctrl+Shift+M: Toggle feed on/off
- Ctrl+Shift+P: Pause/resume
- Ctrl+Shift+S: Take snapshot

## Troubleshooting

### The feed is upside down or mirrored

Your microscope optics may invert the image. In Settings:
1. Go to Display section
2. Use Rotation (0°, 90°, 180°, 270°) to correct orientation
3. Use Flip (Horizontal/Vertical) if the image is mirrored

### My microscope's light doesn't turn on

Micround only controls the camera feed, not microscope lighting. Check:
- The microscope's power supply
- The microscope's lighting controls (usually a wheel or button)
- USB power (some lights need more power than a laptop can provide)

### Settings don't persist after restart

Settings are saved to a configuration file. If settings don't persist:
- Check you have write permission to your user folder
- Make sure you click "Apply" or "OK" in Settings
- Check for antivirus software blocking the config file

### Where are my snapshots saved?

Snapshots are saved to:
- **Windows:** `C:\Users\YourName\Pictures\Micround\`
- **macOS:** `~/Pictures/Micround/`
- **Linux:** `~/Pictures/Micround/`

---

Didn't find your answer? Check the [Troubleshooting Guide](TROUBLESHOOTING.md) or open an issue on GitHub.
