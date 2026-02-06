# Troubleshooting Guide

Solutions to common issues with Micround.

## Camera Issues

### Camera Not Detected

**Symptoms:**
- No cameras appear in the device dropdown
- "No cameras found" message

**Solutions:**

1. **Check physical connection**
   - Unplug and replug the USB cable
   - Try a different USB port
   - Avoid USB hubs - connect directly to your computer

2. **Check if camera is in use**
   - Close other applications that might be using the camera
   - Common culprits: video conferencing apps, streaming software, other camera apps

3. **Verify camera works**
   - Test the camera with another application
   - Windows: Camera app
   - macOS: Photo Booth
   - Linux: Cheese or `ffplay /dev/video0`

4. **Platform-specific checks:**

   **Windows:**
   - Open Device Manager and check if the camera appears under "Cameras" or "Imaging devices"
   - Look for yellow warning triangles indicating driver issues
   - Try updating camera drivers

   **macOS:**
   - Open System Settings → Privacy & Security → Camera
   - Ensure Micround has camera permission
   - Try the camera in Photo Booth to verify it works

   **Linux:**
   - Check if the camera device exists: `ls /dev/video*`
   - Ensure your user is in the `video` group: `groups $USER`
   - Add yourself if needed: `sudo usermod -a -G video $USER` (logout required)

### Camera Detected But No Feed

**Symptoms:**
- Camera appears in dropdown
- Selecting it shows black or frozen image

**Solutions:**

1. **Resolution mismatch**
   - Try a lower resolution (720p or 480p)
   - Some cameras don't support all resolutions

2. **Framerate issue**
   - Try reducing framerate to 15fps
   - Some USB 2.0 ports can't handle high framerate at high resolution

3. **Camera needs initialization**
   - Some microscopes need a few seconds to "warm up"
   - Wait 5-10 seconds after selecting the camera

4. **USB power issues**
   - Microscopes can be power-hungry
   - Try a powered USB hub
   - Avoid long USB extension cables

### Camera Feed Is Flickering

**Symptoms:**
- Image flashes on and off
- Feed keeps starting and stopping

**Solutions:**

1. **Unstable USB connection**
   - Check cable connection
   - Try a different USB cable
   - Avoid moving the cable while running

2. **Power management**
   - Disable USB selective suspend (Windows)
   - System Preferences → Energy Saver → disable "Put hard disks to sleep" (macOS)

3. **Competing applications**
   - Ensure only Micround is using the camera

## Display Issues

### Wallpaper Not Showing

**Symptoms:**
- Feed runs but doesn't appear as wallpaper
- Original wallpaper still shows

**Solutions:**

1. **Windows:**
   - Right-click desktop → Personalize → Background
   - Make sure you're not using a slideshow
   - Try restarting Micround as administrator (one time only)

2. **macOS:**
   - Restart Micround
   - Some third-party wallpaper apps may conflict

3. **Linux:**
   - Micround currently requires X11
   - If using Wayland, switch to an X11 session
   - Some desktop environments (e.g., Gnome 40+) have limited wallpaper support

### Wrong Monitor

**Symptoms:**
- Feed appears on wrong display

**Solutions:**

1. Open Settings → Display → Target
2. Select the correct monitor
3. Click Apply

If monitors aren't detected correctly:
- Disconnect and reconnect the monitor
- Click "Refresh" in Settings

### Image Quality Issues

**Symptoms:**
- Blurry or pixelated image
- Colors look wrong

**Solutions:**

1. **Blurry image:**
   - Adjust microscope focus (physical knob)
   - Increase resolution in Settings
   - Use "Fit" scaling mode instead of "Stretch"

2. **Wrong colors:**
   - Some microscopes need white balance adjustment
   - Check microscope's own software for color settings
   - LED lighting color temperature affects appearance

3. **Pixelated/blocky:**
   - Increase capture resolution
   - Use a microscope with higher resolution sensor

## Performance Issues

### High CPU Usage

**Symptoms:**
- CPU usage above 20%
- System feels sluggish

**Solutions:**

1. Lower resolution to 720p or 480p
2. Reduce framerate to 15fps
3. Check for other processes using the camera
4. Ensure GPU acceleration is working (check if your graphics drivers are up to date)

### High Memory Usage

**Symptoms:**
- Memory usage grows over time
- System becomes slow after hours of use

**Solutions:**

1. Try restarting Micround
2. Update to the latest version (may contain memory leak fixes)
3. Report the issue with your system details if it persists

### Feed Lag/Delay

**Symptoms:**
- Movement on microscope appears delayed on screen
- More than 0.5 second delay

**Solutions:**

1. Close other applications using the camera
2. Use USB 3.0 port if available
3. Lower resolution
4. Check USB cable quality (use shielded cable)

## Platform-Specific Issues

### Windows

**SmartScreen warning on install:**
- The installer isn't signed (costs ~$400/year)
- Click "More info" → "Run anyway"
- This is safe for official downloads

**Feed doesn't show on Windows 11:**
- Windows 11 changed wallpaper handling
- Ensure you have the latest Micround version
- Try running as administrator once

### macOS

**"Micround cannot be opened" error:**
- Right-click the app → Open → Open
- Or: System Settings → Privacy & Security → Open Anyway

**Camera permission not working:**
1. Open System Settings → Privacy & Security → Camera
2. Find Micround in the list
3. Toggle it off, then on again
4. Restart Micround

**App Nap causing issues:**
- Micround should prevent App Nap automatically
- If feed stutters when app is in background, report the issue

### Linux

**Wayland not supported:**
- Switch to X11 session at login
- Or use Xwayland (limited support)

**Feed doesn't show:**
- Check if your compositor supports wallpaper changes
- Try with a different desktop environment

**Permission denied on /dev/video0:**
```bash
sudo usermod -a -G video $USER
```
Then log out and back in.

## Getting More Help

If your issue isn't covered here:

1. Check for updates - many issues are fixed in newer versions
2. Search existing issues on GitHub
3. Open a new issue with:
   - Your OS and version
   - Your microscope model
   - Steps to reproduce the problem
   - Any error messages you see

---

For general usage questions, see [FAQ.md](FAQ.md).
