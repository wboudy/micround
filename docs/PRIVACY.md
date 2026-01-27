# Micround Privacy Design Document

Micround is designed with privacy as a foundational principle. This document establishes the privacy guarantees, coding guidelines, and enforcement mechanisms.

## Core Privacy Promises

These promises are **immutable** and must never be violated:

### 1. No Network Connections
Micround NEVER connects to any server for any reason.
- No telemetry or analytics
- No crash reporting to external services
- No update checks (users download updates manually)
- No cloud sync or backup
- No remote streaming (future feature would require explicit opt-in)

### 2. No Video Recording
Camera frames exist only in memory during display.
- Frames are never written to disk automatically
- No background recording
- No buffer files or temp video files
- **Exception**: User-initiated snapshots save single frames to user-chosen location

### 3. Local-Only Operation
All data stays on the user's machine.
- Settings stored locally in user-accessible config file
- Logs stored locally, rotated, capped at 10MB
- No data leaves the device

## What We Store

| Data | Location | Purpose |
|------|----------|---------|
| Settings | `~/.config/micround/config.toml` | User preferences |
| Logs | `~/.config/micround/logs/` | Debugging (rotated, max 10MB) |
| Snapshots | User-chosen folder | Explicit user captures only |
| Original wallpaper path | In settings file | Restore on exit/crash |

### What Settings Contain
- Selected camera device ID (hardware identifier)
- Display preferences (scaling mode, rotation, etc.)
- UI preferences (startup behavior, hotkeys)
- **Never**: Image data, frame captures, video content

### What Logs Contain
- Timestamps and log levels
- Component names and operations
- Error messages and stack traces
- Performance metrics (fps, latency numbers)
- **Never**: Frame data, pixel values, image content, full file paths with usernames

## What We DON'T Store

- Video recordings of any kind
- Frame captures (except explicit snapshots)
- Usage analytics or telemetry
- Crash dumps with memory contents
- Any data on external servers

## Forbidden APIs

The following must NEVER appear in the Micround codebase:

### Network
```rust
// FORBIDDEN - No network APIs
std::net::TcpStream
std::net::UdpSocket
reqwest::*
hyper::*
tokio::net::*
curl::*
```

### Video Recording
```rust
// FORBIDDEN - No video file writing
// (except the explicit snapshot feature with user confirmation)
ffmpeg::output_file
gstreamer::FileSink
// Any video encoder writing to disk
```

### Telemetry
```rust
// FORBIDDEN - No analytics or telemetry
sentry::*
bugsnag::*
google_analytics::*
// Any "send data home" pattern
```

## Required Patterns

### Frame Buffer Handling
```rust
// REQUIRED: Clear sensitive data after use
impl Drop for FrameBuffer {
    fn drop(&mut self) {
        // Zero out frame data before deallocation
        self.data.iter_mut().for_each(|b| *b = 0);
    }
}
```

### Logging Safety
```rust
// CORRECT: Log metadata only
tracing::info!(
    camera_id = %device.id,
    resolution = ?settings.resolution,
    "Camera connected"
);

// FORBIDDEN: Never log frame content
// tracing::debug!(frame_data = ?frame.pixels); // NO!
```

### Path Sanitization
```rust
// CORRECT: Sanitize paths in logs
fn log_safe_path(path: &Path) -> String {
    // Replace username with placeholder
    path.to_string_lossy()
        .replace(&std::env::var("USER").unwrap_or_default(), "<user>")
}
```

## Enforcement Mechanisms

### 1. Code Review Checklist
Every PR must confirm:
- [ ] No new network dependencies added
- [ ] No video file writing (except snapshot feature)
- [ ] No telemetry or analytics code
- [ ] Logs contain no frame data or sensitive paths
- [ ] Frame buffers cleared after use

### 2. CI Static Analysis
```bash
# Check for forbidden patterns (run in CI)
! grep -r "reqwest\|hyper\|TcpStream\|UdpSocket" src/
! grep -r "sentry\|bugsnag\|analytics" src/
```

### 3. Dependency Audit
```bash
# Verify no network-capable dependencies sneak in
cargo tree | grep -E "reqwest|hyper|curl"  # Should be empty
```

## User Verification

Users can verify our privacy claims:

### Network Verification
```bash
# Monitor network during operation
# macOS
sudo tcpdump -i any -n "port not 22"

# Linux
sudo tcpdump -i any -n "port not 22"

# Windows
netstat -b  # Shows connections per process
```

### File System Verification
```bash
# Monitor file writes during operation
# macOS
sudo fs_usage -w -f filesystem | grep micround

# Linux
inotifywait -m -r ~/.config/micround/
```

## Data Deletion

To completely remove all Micround data:

### macOS / Linux
```bash
rm -rf ~/.config/micround/
rm -rf ~/.local/share/micround/  # If used
```

### Windows
```powershell
Remove-Item -Recurse "$env:APPDATA\micround"
Remove-Item -Recurse "$env:LOCALAPPDATA\micround"
```

## Camera Access Transparency

### What Camera Access Means
- Micround reads frames from your camera
- Frames are displayed on your desktop
- Frames are discarded immediately after display
- We cannot see your camera - only your computer can

### Platform Permissions
| Platform | Permission Required | User Control |
|----------|-------------------|--------------|
| Windows | None (implicit for native apps) | Camera privacy settings |
| macOS | Camera permission (TCC) | System Preferences > Privacy |
| Linux | `/dev/video*` access | `video` group membership |

## Privacy by Design Principles

1. **Minimize data**: Collect only what's needed (camera ID, settings)
2. **Encrypt at rest**: Config file can use OS keychain for sensitive data
3. **Fail closed**: On error, stop capture rather than degrade privacy
4. **User control**: User can always stop, and stopping restores wallpaper
5. **Transparency**: Open source allows verification of all claims

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-27 | Initial privacy design document |
