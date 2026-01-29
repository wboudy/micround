# User-Facing Error Messages Design

This document defines the design principles, message templates, and recovery guidance for all user-facing errors in Micround.

## Design Principles

### 1. Tell Users What Happened (Plain Language)
- Use everyday words, not technical jargon
- Be specific about what failed
- Don't expose internal error codes unless helpful for support

### 2. Tell Users What to Do (Actionable Guidance)
- Every error should have a clear next step
- Use action verbs: "Connect", "Check", "Close", "Grant"
- Include buttons for common actions when possible

### 3. Don't Blame the User
- Use "Unable to" instead of "You failed to"
- Focus on the situation, not the user's actions
- Keep tone friendly but professional

### 4. Don't Use Technical Jargon
- Avoid: HRESULT, EBUSY, errno, mutex, thread
- Use: camera, display, settings, permission

## Error Categories

### Recoverable (Auto-Retry)
User sees: Brief status update or nothing
System action: Retry automatically, log at debug level

### User-Actionable
User sees: Notification with clear guidance
System action: Log at warn level, wait for user action

### Fatal
User sees: Dialog with explanation, may need to restart
System action: Log at error level, attempt safe shutdown

## Error Message Templates

### Camera Errors

| Scenario | Message | Action |
|----------|---------|--------|
| No cameras found | "No cameras found. Connect a USB camera and click Refresh." | [Refresh] [Help] |
| Camera not found | "Camera '[name]' is not available. Check the connection and click Retry." | [Retry] [Select Different Camera] |
| Camera in use | "Camera is being used by another application. Close other apps using the camera and try again." | [Retry] [Help] |
| Permission denied | "Camera access was denied. Grant camera permission in system settings." | [Open Settings] [Help] |
| Camera disconnected | "Camera disconnected. Waiting to reconnect..." | (Auto-retry indicator) |
| Format negotiation failed | "Unable to use this camera's video format. Try a different camera or resolution." | [Settings] [Select Camera] |
| Capture timeout | "Camera is not responding. Reconnecting..." | (Auto-retry indicator) |

### Display Errors

| Scenario | Message | Action |
|----------|---------|--------|
| Display not found | "Display '[name]' is no longer available. Feed moved to primary display." | [OK] [Settings] |
| Display disconnected | "Display disconnected. Feed moved to [Primary Display]." | [OK] |
| Resolution changed | "Display resolution changed. Adjusting feed..." | (No action needed) |
| Surface creation failed | "Unable to create display surface. Check your graphics drivers." | [Help] |
| GPU error | "A graphics error occurred. Update your graphics drivers and restart." | [Restart] [Help] |
| Wallpaper integration failed | "Unable to set wallpaper. Your desktop environment may not support this feature." | [Help] |

### Configuration Errors

| Scenario | Message | Action |
|----------|---------|--------|
| Config read failed | "Unable to read settings. Using default configuration." | [OK] [Reset Settings] |
| Config write failed | "Unable to save settings. Check that you have write permissions." | [Retry] [Help] |
| Config invalid | "Settings file is corrupted. Using default configuration." | [OK] [Reset Settings] |
| Config not found | "Settings file not found. Using default configuration." | [OK] |

### System Errors

| Scenario | Message | Action |
|----------|---------|--------|
| Autostart IO error | "Unable to change autostart settings. Check your permissions." | [Help] |
| Autostart not supported | "Autostart is not available. Add Micround to startup manually." | [Help] |
| Path not found | "Unable to find required directory. Check system configuration." | [Help] |
| Directory creation failed | "Unable to create application directory. Check write permissions." | [Help] |

### Feed/Processing Errors

| Scenario | Message | Action |
|----------|---------|--------|
| Feed frozen | "Feed appears frozen." | [Restart Feed] |
| Frame processing error | "Error processing video frame. Skipping..." | (No action, auto-continue) |
| Memory pressure | "System is low on memory. Performance may be affected." | [OK] |

## Recovery Actions

### Primary Actions (Buttons)
- **[Retry]** - Attempt the operation again
- **[Restart Feed]** - Stop and restart the camera feed
- **[Settings]** - Open the settings panel
- **[Select Camera]** - Open camera selection
- **[Help]** - Open help documentation
- **[OK]** - Dismiss the notification
- **[Restart]** - Restart the application

### Secondary Actions (Links)
- "Open Settings" - System settings for permissions
- "Learn more" - Link to documentation
- "Contact support" - Support resources

## Implementation Guidelines

### Message Structure
```rust
pub fn user_message(&self) -> String {
    match self {
        // Format: What happened + What to do
        Self::PermissionDenied(_) => {
            "Camera access was denied. Grant camera permission in system settings.".into()
        }
        // ...
    }
}
```

### Notification Display
1. For recoverable errors: Status bar or toast (auto-dismiss)
2. For user-actionable errors: Persistent notification with action buttons
3. For fatal errors: Modal dialog requiring acknowledgment

### Error Context for Support
- Technical details available via "Show Details" expansion
- Error code displayed in small text for support purposes
- Full error logged internally with timestamp and context

### Localization Readiness
- All user messages defined in centralized location
- Use format strings for dynamic content: `format!("Camera '{}' not found", name)`
- Avoid string concatenation in messages

## Testing Checklist

For each error scenario:
- [ ] Trigger the error condition
- [ ] Verify message is clear and helpful
- [ ] Verify action buttons work correctly
- [ ] Verify technical details are logged
- [ ] Test with non-technical user (if possible)

## Error Display Priority

When multiple errors occur:
1. Fatal errors take precedence (modal dialog)
2. User-actionable errors queue in notification area
3. Recoverable errors show briefly or not at all
4. Group related errors (e.g., multiple camera errors)

## Anti-Patterns to Avoid

1. **Silent failure**: Always notify for user-actionable errors
2. **Technical dumps**: Don't show stack traces to users
3. **Vague messages**: Be specific about what failed
4. **No guidance**: Always suggest next steps
5. **Blame language**: Never imply user fault
6. **Overnotification**: Don't spam users with every timeout
