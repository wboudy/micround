# Micround: Live Microscope Desktop Wallpaper System
## Comprehensive Project Plan (Enhanced Hybrid Edition)

---

## 0. Starting Point & Repository State

### Current State
This project is starting from a **completely empty repository**. The following initialization has been completed:

```
micround/
├── .git/          # Git repository initialized (git init)
├── .beads/        # Beads version control initialized (bd init)
└── docs/
    └── PROJECT_PLAN.md   # This document
```

### What Exists
- **Git repository**: Initialized but with no commits yet
- **Beads**: Initialized for structured version control
- **This plan**: The sole artifact defining what we're building

### What Doesn't Exist Yet
- No source code
- No build configuration
- No dependencies defined
- No CI/CD pipeline
- No tests
- No documentation beyond this plan

### Implications for Development
1. **Greenfield project**: All architectural decisions are open; no legacy constraints
2. **Language/framework choice**: Not yet determined (see Open Questions in Section 15)
3. **First milestone**: Prototype phase will establish the core technology stack
4. **Build from scratch**: Every component described in this plan must be implemented from zero

### Recommended First Steps (Post-Plan Approval)
1. Choose programming language and UI framework
2. Set up build system and dependency management
3. Create initial project structure
4. Implement camera capture proof-of-concept
5. Implement wallpaper rendering proof-of-concept (Windows first)
6. Integrate into working prototype

---

## 1. Project Summary

### What We're Building
A desktop application that captures live video from a microscope camera and displays it as the system's desktop wallpaper in real time. The user places a sample under the microscope, and their desktop background becomes a continuously updating live view of the microscopic world—creating an ambient, living desktop experience where bacteria, crystals, cells, and water samples become the user's continuous visual environment.

### Definition of Success

| Timeframe | Success Criteria | Measurable Target |
|-----------|------------------|-------------------|
| **Day-1** | Plug-and-play experience | Camera → live wallpaper within 10 seconds of app launch |
| **Day-1** | Imperceptible latency | Motion on microscope stage appears on wallpaper with <100ms delay |
| **Week-1** | Invisible operation | Runs stably for 8+ hours without user intervention |
| **Week-1** | Resource-invisible | Users don't notice performance impact on other work |
| **Month-1** | Set-and-forget | Stable enough to leave running indefinitely; survives sleep/wake |
| **Month-1** | Cross-platform parity | Core experience works identically on Windows, macOS, and Linux |

### Primary Use Cases

| Priority | Use Case | User Persona |
|----------|----------|--------------|
| **Must** | Educational/hobbyist: observe pond water, plant cells, crystals as ambient desktop art | Science enthusiast, student |
| **Must** | Science communication: live demo backdrop during presentations or streams | Educator, content creator |
| **Should** | Laboratory ambient display: passive monitoring of long-running experiments | Lab technician |
| **Should** | Hobbyist observation: crystal growth, fermentation, yeast activity | Hobbyist |
| **Could** | Art installation: generative/biological desktop aesthetic | Digital artist |

### Non-Goals (Explicit Exclusions)
- **Not a microscope control app**: No stage movement, focus control, or illumination adjustment
- **Not a recording/capture app**: Primary function is live display, not archival (recording is future backlog)
- **Not a network streaming app**: Local display only; network streaming is future scope
- **Not a scientific measurement tool**: No calibrated rulers, cell counting, or analysis features in v1
- **Not a multi-camera dashboard**: Single active source only for v1

---

## 2. User Stories & Workflows

### User Stories

#### Novice User Stories
| ID | Story | Acceptance Criteria |
|----|-------|---------------------|
| **US-N1** | As a hobbyist, I want to plug in my USB microscope and have the app auto-detect it, so I don't need to configure anything. | Camera appears in list within 3 seconds of connection |
| **US-N2** | As a student, I want the live feed to automatically fill my desktop without black bars or distortion, so it looks professional. | Default scaling mode produces visually pleasing result |
| **US-N3** | As a casual user, I want a simple on/off toggle in the system tray, so I can quickly return to my normal wallpaper. | Single click stops feed and restores original wallpaper |
| **US-N4** | As a presenter, I want the feed to continue when I open full-screen apps, so my presentation isn't disrupted. | Feed continues rendering (invisibly) during full-screen apps |
| **US-N5** | As a first-time user, I want clear feedback if the camera isn't working, so I know what to fix. | Error states show actionable messages |

#### Power User Stories
| ID | Story | Acceptance Criteria |
|----|-------|---------------------|
| **US-P1** | As a power user, I want to select which camera to use when multiple are connected, so I can switch between microscopes. | Camera dropdown with friendly names |
| **US-P2** | As a multi-monitor user, I want to choose which display shows the feed (or all of them), so I can dedicate one monitor to microscopy. | Per-monitor selection with "All" option |
| **US-P3** | As a streamer, I want to add a subtle overlay (timestamp, scale bar placeholder, custom text), so viewers have context. | Configurable overlay with position options |
| **US-P4** | As a lab user, I want to freeze/pause the feed temporarily without stopping capture, so I can examine a moment while the sample continues. | Pause shows frozen frame; resume continues from live |
| **US-P5** | As a long-session user, I want the app to gracefully recover from sleep/wake cycles without manual restart. | 100% automatic recovery from sleep/wake |
| **US-P6** | As a power user, I want to save named session presets (camera + crop + rotation + monitor layout) and switch instantly. | Preset save/load with keyboard shortcuts |
| **US-P7** | As a user, I want to compare frozen frame against live feed with a fade overlay to spot subtle changes. | One-key freeze + compare mode |
| **US-P8** | As a power user, I want a floating mini-preview window for focused inspection while keeping the wallpaper live. | Dual-view mode with resizable preview |
| **US-P9** | As a laptop user, I want the app to auto-adjust quality based on system load so it stays responsive. | Adaptive quality keeps fps stable without manual tuning |
| **US-P10** | As a privacy-conscious user, I want an instant panic stop to restore my wallpaper. | Global hotkey restores wallpaper within 1 second |

### Day-1 Workflow (First Use)
```
1. User installs application
2. User connects USB microscope camera
3. User launches application
4. First-run dialog appears: "This app displays your camera feed on your desktop.
   Nothing is recorded or sent anywhere. [Got it]"
5. App auto-detects camera, shows preview in settings window
6. User clicks "Set as Wallpaper" (single primary action)
7. Desktop background immediately shows live feed
8. User adjusts sample under microscope, sees real-time response (<100ms)
9. User adjusts scale/rotation via tray menu if needed
10. User minimizes settings window; feed continues via system tray icon
11. User clicks tray icon → "Stop" to restore previous wallpaper
```

### Day-30 Workflow (Long-Running Reliability)
```
1. App launches at system startup (user-configured)
2. Automatically connects to last-used camera
3. Resumes live wallpaper without user interaction
4. Tray tooltip shows: "Live: 1920x1080 @ 30fps | Running: 47h 23m | 0 reconnects"
5. Survives multiple sleep/wake cycles throughout the day
6. Handles camera temporary disconnects (USB reset, hub issues):
   - Shows "Reconnecting..." overlay on wallpaper
   - Auto-reconnects with exponential backoff
   - Resumes seamlessly when device returns
7. If camera is permanently removed:
   - Shows static fallback (user's original wallpaper)
   - Tray notification: "Camera disconnected. Click to reconnect."
   - Keeps polling for device return in background
8. Weekly: user can review simple local log if curious (no telemetry sent anywhere)
```

---

## 3. System Requirements

### Functional Requirements

| ID | Requirement | Priority | Notes |
|----|-------------|----------|-------|
| **FR-01** | Enumerate and select from available video capture devices | Must | Include friendly names; auto-select if single camera |
| **FR-02** | Display live camera feed as desktop wallpaper | Must | Core functionality |
| **FR-03** | Support common resolutions (640x480 to 4K) | Must | Handle mismatch with display gracefully |
| **FR-04** | Scale/crop modes: Fit, Fill, Stretch, Center | Must | User-selectable per display |
| **FR-05** | Rotate feed: 0°, 90°, 180°, 270° | Should | Common for microscope orientation |
| **FR-06** | Flip feed horizontally/vertically | Should | Mirror correction for optics |
| **FR-07** | Pause/freeze current frame while capture continues | Should | Examine without stopping |
| **FR-08** | Resume from pause | Should | Seamless continuation |
| **FR-09** | Capture snapshot to file or clipboard | Should | Clipboard + session folder; hotkey; timestamped filename and metadata |
| **FR-10** | Contextual overlays: scale bar, timestamp, status, objective label | Should | Minimal, auto-hide on mouse move; DPI-aware; calibration optional |
| **FR-11** | Fallback to static wallpaper on failure | Must | Graceful degradation |
| **FR-12** | Restore original wallpaper on app exit | Must | Clean shutdown |
| **FR-13** | System tray presence with context menu | Must | Minimal UI footprint |
| **FR-14** | Persist settings across sessions | Must | Remember camera, scaling, position |
| **FR-15** | Launch at system startup (opt-in) | Should | Convenience for dedicated setups |
| **FR-16** | Multi-monitor: select target display(s) | Must | Independent per-monitor control; DPI-aware scaling |
| **FR-17** | Handle display configuration changes | Must | Resolution, arrangement, DPI changes |
| **FR-18** | Session presets: save/load named configurations | Should | Auto-recall by camera + monitor layout; fast switching |
| **FR-19** | Freeze + compare mode: fade or blink against live | Should | Opacity slider + A/B toggle to spot subtle changes |
| **FR-20** | Floating preview/inspector window (dual-view mode) | Should | Optional zoom/loupe and pixel grid without disrupting wallpaper |
| **FR-21** | Smart auto-crop: one-click "Best View" centering | Should | Detects black borders; stable framing with lock option |
| **FR-22** | Adaptive quality mode with manual lock | Should | Auto-tunes fps/resolution with hysteresis and user priority (latency/quality/battery) |
| **FR-23** | Auto-recovery watchdog | Must | Self-heals disconnects/sleep; clear user messaging; logs incidents |
| **FR-24** | Privacy/status indicators + panic stop | Must | Always-visible status; instant restore hotkey; optional on-desktop indicator |

### Non-Functional Requirements

| ID | Requirement | Target | Stretch | Measurement Method |
|----|-------------|--------|---------|-------------------|
| **NFR-01** | End-to-end latency | ≤100ms p95 | ≤80ms p50 | High-speed camera + millisecond timer |
| **NFR-02** | Frame rate floor | ≥24 fps sustained | ≥30 fps | Frame counter overlay during test |
| **NFR-03** | Frame rate target | 30 fps | 60 fps | Match camera native rate |
| **NFR-04** | CPU usage (idle desktop, 1080p feed) | ≤10% single core | ≤5% | OS task manager monitoring |
| **NFR-05** | GPU usage (idle desktop, 1080p feed) | ≤15% | ≤10% | GPU monitoring tools |
| **NFR-06** | Memory footprint | ≤200 MB resident | ≤150 MB | OS memory monitoring |
| **NFR-07** | Crash-free operation | ≥72 hours | ≥168 hours | Soak testing |
| **NFR-08** | Startup to live wallpaper | ≤5 seconds | ≤3 seconds | Stopwatch from app launch |
| **NFR-09** | Sleep/wake recovery | ≤5 seconds | ≤3 seconds | To live feed restored |
| **NFR-10** | Camera reconnect time | ≤3 seconds | ≤2 seconds | After device returns |
| **NFR-11** | No network access | Zero outbound connections | - | Network monitoring |
| **NFR-12** | No persistent frame storage | Zero disk writes of video data | - | Filesystem monitoring |

### Adaptive Quality System

The application shall implement an **Adaptive Quality Mode** with hysteresis and a user lock option to prevent oscillation and allow **latency‑first**, **quality‑first**, or **battery‑first** behavior. It automatically adjusts performance parameters based on system load:

| System State | Frame Rate | Resolution | Processing |
|--------------|------------|------------|------------|
| **Idle** (CPU <30%, GPU <30%) | 30 fps | Native | Full overlays |
| **Light load** (CPU 30-60%) | 24 fps | Native | Full overlays |
| **Moderate load** (CPU 60-80%) | 20 fps | Native | Simplified overlays |
| **Heavy load** (CPU >80% or thermal throttling) | 15 fps | 75% scale | Minimal processing |
| **Eco mode** (user-selected for laptops) | 15 fps | 75% scale | Minimal processing |

**Manual lock behavior**: When the user selects a fixed quality tier, the system maintains that target unless stability is at risk, then surfaces a gentle warning rather than silently degrading.

**Priority behavior**:
- **Latency‑first**: prioritize minimal buffering, prefer frame drops over latency growth.
- **Quality‑first**: prioritize resolution/visual fidelity with mild buffering tolerance.
- **Battery‑first**: lower fps/resolution early; prefer GPU‑off where possible.

### High-Impact Feature Set (Refined)

These 10 features are the highest‑ROI enhancements: strong user value with moderate implementation burden. They should guide scope and prioritization.

| # | Feature | User Value | Priority | Related Requirements |
|---|---------|-----------|----------|----------------------|
| **1** | **Adaptive Quality + priority modes** | Smooth motion without tweaking; user can prefer latency, quality, or battery | Should | FR-22, NFR-01..NFR-05 |
| **2** | **Smart framing (auto‑crop + lock)** | One‑click “Best View” removes borders with stable, non‑jittery framing | Should | FR-21 |
| **3** | **Freeze + compare (A/B)** | Instantly spot subtle changes via fade/blink toggle | Should | FR-19 |
| **4** | **Contextual overlays** | Scale/timestamp/status with auto‑hide to avoid clutter | Should | FR-10, FR-24 |
| **5** | **Session presets + auto‑recall** | Switch setups instantly; auto‑apply per camera/monitor | Should | FR-18 |
| **6** | **Auto‑recovery + diagnostics** | Self‑heals disconnects/sleep and explains issues clearly | Must | FR-23 |
| **7** | **Inspector window (zoom/loupe)** | Focused inspection without disrupting wallpaper | Should | FR-20 |
| **8** | **Instant snapshots + hotkey** | Zero‑friction capture with metadata | Should | FR-09 |
| **9** | **Multi‑monitor intelligence** | Per‑monitor control with DPI‑aware scaling and hot‑plug handling | Must | FR-16, FR-17 |
| **10** | **Privacy + panic stop** | Always‑clear “Live” status and instant restore | Must | FR-24 |

---

## 4. Constraints & Assumptions

### Camera Types Supported

| Type | Connection | v1 Support | Notes |
|------|------------|------------|-------|
| **UVC Webcam** | USB Video Class | **Yes** | Most microscope cameras; plug-and-play; baseline |
| **HDMI Capture Device** | USB (appears as UVC) | **Yes** | For HDMI-output microscopes; higher latency typical |
| **Vendor SDK Camera** | USB + proprietary driver | **No** (v2+) | Architecture should not preclude future support |

**v1 Assumption**: Target UVC cameras exclusively. This covers 90%+ of consumer/prosumer microscope cameras and HDMI capture dongles.

### Operating System Targets

| OS | Version Target | Desktop Environment | Key Differences |
|----|---------------|---------------------|-----------------|
| **Windows** | 10 (1903+), 11 | Native desktop | Most flexible wallpaper manipulation; DWM always active |
| **macOS** | 12 (Monterey)+ | Native + Spaces | Stricter security; desktop picture API limitations; notarization required; App Nap concerns |
| **Linux** | Ubuntu 22.04+, Fedora 38+ | X11 (primary), Wayland (experimental) | Fragmented DE landscape; X11 root window viable; Wayland compositor-dependent |

**Recommended Path**: Ship Windows first (largest audience, most predictable), macOS second, Linux X11 third. Wayland as future work.

### Monitor Configurations

| Configuration | Support Level | Handling |
|---------------|---------------|----------|
| Single monitor | Must | Default case; straightforward |
| Dual monitor (same DPI) | Must | Independent or mirrored; user choice |
| Multi-monitor (mixed DPI) | Should | Render at highest DPI, scale down for others |
| Virtual desktops / Spaces | Should | Feed persists across desktop switches (OS-dependent) |
| Portrait orientation | Should | Handle via rotation setting |
| External display connect/disconnect | Must | Detect and adapt layout; don't crash |

**Assumption**: User has at least one display ≥1280x720.

---

## 5. Architecture (Conceptual)

### ASCII Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                               MICROUND SYSTEM                                     │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                   │
│  ┌─────────────────┐    ┌──────────────────────┐    ┌─────────────────────────┐  │
│  │     CAMERA      │    │    VIDEO INGEST      │    │   FRAME PROCESSING      │  │
│  │   (Hardware)    │───▶│       LAYER          │───▶│      PIPELINE           │  │
│  │                 │    │                      │    │                         │  │
│  │  • UVC Device   │    │  • Device enumeration│    │  • Color space convert  │  │
│  │  • HDMI Capture │    │  • Stream management │    │  • Scale / crop / rotate│  │
│  │                 │    │  • Frame capture     │    │  • Flip H/V             │  │
│  └─────────────────┘    │  • Format negotiation│    │  • Overlay compositing  │  │
│                         │  • Reconnect logic   │    │  • Adaptive quality     │  │
│                         └──────────────────────┘    └───────────┬─────────────┘  │
│                                                                  │                │
│                                                                  ▼                │
│  ┌────────────────────────────────────────────────────────────────────────────┐  │
│  │                    WALLPAPER RENDERING / DESKTOP INTEGRATION               │  │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐               │  │
│  │  │    Windows     │  │     macOS      │  │     Linux      │               │  │
│  │  │    Backend     │  │    Backend     │  │    Backend     │               │  │
│  │  │                │  │                │  │                │               │  │
│  │  │ • WorkerW      │  │ • NSWindow at  │  │ • X11 root     │               │  │
│  │  │   injection    │  │   desktop level│  │   window       │               │  │
│  │  │ • DirectComp   │  │ • Quartz       │  │ • EWMH hints   │               │  │
│  │  │   (fallback)   │  │   compositor   │  │ • Wayland      │               │  │
│  │  │                │  │                │  │   layer-shell  │               │  │
│  │  └────────────────┘  └────────────────┘  └────────────────┘               │  │
│  └────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                   │
│  ┌──────────────────────────────────────────────────────────────────────────┐    │
│  │                        SHARED STATE / EVENT BUS                           │    │
│  │  • State machine coordination  • Frame dropping policy  • Jank detection  │    │
│  └──────────────────────────────────────────────────────────────────────────┘    │
│         │                         │                         │                     │
│         ▼                         ▼                         ▼                     │
│  ┌──────────────────┐  ┌─────────────────────┐  ┌────────────────────────────┐  │
│  │  CONTROL         │  │  CONFIGURATION &    │  │  LOGGING, DIAGNOSTICS &    │  │
│  │  SURFACE         │  │  PERSISTENCE        │  │  AUTO-RECOVERY ENGINE      │  │
│  │                  │  │                     │  │                            │  │
│  │ • System tray    │  │ • Settings file     │  │ • Local log files          │  │
│  │ • Settings window│  │   (JSON/TOML)       │  │ • Performance metrics      │  │
│  │ • Keyboard shorts│  │ • Session presets   │  │ • Frame timing histogram   │  │
│  │ • Status display │  │ • "Running" state   │  │ • Watchdog / reconnect     │  │
│  │ • Preview window │  │   for crash detect  │  │ • Thermal monitoring       │  │
│  └──────────────────┘  └─────────────────────┘  └────────────────────────────┘  │
│                                                                                   │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Inputs | Outputs |
|-----------|----------------|--------|---------|
| **Video Ingest Layer** | Camera discovery, connection management, raw frame acquisition, reconnection logic | OS device events, user camera selection | Raw frames (native format), device status events |
| **Frame Processing Pipeline** | Transform raw frames to display-ready format; adaptive quality scaling | Raw frames, transform settings, system load | Processed RGBA frames, frame metadata |
| **Wallpaper Renderer** | OS-specific desktop integration, frame display, multi-monitor handling | Processed frames, display configuration | Rendered desktop background |
| **Control Surface** | User interaction, status display, preview window | User input events | Commands to other components |
| **Configuration & Persistence** | Settings storage/retrieval, session presets, crash state tracking | Settings changes | Persisted configuration |
| **Auto-Recovery Engine** | Watchdog, reconnection, state restoration, thermal monitoring | Events from all components | Recovery actions, log entries |
| **Logging/Diagnostics** | Local troubleshooting data, performance metrics, jank detection | Events from all components | Log files (local only, rotated, capped at 10MB) |

### Key Internal Interfaces

```
VIDEO_INGEST → FRAME_PROCESSING
  Frame {
    data: byte[]
    format: PixelFormat (MJPEG, YUY2, RGB24, etc.)
    width, height: int
    timestamp: monotonic_ns
    sequence: int  // For drop detection
  }

  DeviceStatus {
    connected: bool
    deviceId: string
    deviceName: string
    error?: string
    capabilities: Resolution[]
  }

FRAME_PROCESSING → WALLPAPER_RENDERER
  ProcessedFrame {
    data: byte[] (RGBA)
    width, height: int
    target_displays: DisplayId[]
    overlays: Overlay[]
  }

CONTROL_SURFACE → ALL_COMPONENTS
  Command {
    type: Start | Stop | Pause | Resume | Configure | Snapshot | LoadPreset | SavePreset
    payload: variant
  }

ALL_COMPONENTS → LOGGING
  LogEvent {
    level: Debug | Info | Warn | Error
    component: string
    message: string
    context: map<string, any>
    timestamp: monotonic_ns
  }

AUTO_RECOVERY → ALL_COMPONENTS
  RecoveryEvent {
    type: CameraLost | CameraFound | SleepDetected | WakeDetected | ThermalThrottle
    action: Reconnect | Fallback | Restore | Throttle
  }
```

---

## 6. OS Integration Strategies (Options & Tradeoffs)

### Windows

| Strategy | Description | Pros | Cons | Recommendation |
|----------|-------------|------|------|----------------|
| **A. Window Behind Desktop Icons** | Create borderless window, position behind icon layer using WorkerW/SHELLDLL_DefView hierarchy | Low latency, full control, well-documented community approach | Relies on undocumented shell internals; may break with Windows updates | **Primary** |
| **B. DirectComposition Overlay** | Use DirectComposition to composite onto desktop | Hardware-accelerated, modern, Microsoft-supported | Complex setup, may conflict with DWM policies | **Fallback** |
| **C. Wallpaper File Replacement** | Write frames to temp file, call SystemParametersInfo repeatedly | Simple, uses official API | High latency (disk I/O), flicker, SSD wear, not designed for video | Emergency only |
| **D. Custom Shell Replacement** | Replace explorer.exe desktop component | Full control | Extreme complexity, user friction, compatibility nightmare | Not viable |

**Recommended Path (Windows)**: Start with Strategy A (WorkerW injection) for low latency. Fall back to Strategy B (DirectComposition) if injection proves unstable. Keep Strategy C as emergency fallback.

**Windows-Specific Pitfalls**:
- WorkerW hierarchy changes between Windows 10 builds (test matrix critical)
- Full-screen exclusive apps may disrupt window ordering
- UAC elevation not required, but installer may need admin for startup registration
- Screensaver activation: need to handle gracefully (pause feed, resume after)
- High-DPI scaling requires careful coordinate math
- Antivirus may flag injection behavior (code signing helps)

---

### macOS

| Strategy | Description | Pros | Cons | Recommendation |
|----------|-------------|------|------|----------------|
| **A. NSWindow at Desktop Level** | Create window with `windowLevel` below normal windows, above desktop | Relatively straightforward, decent control | Desktop icons render above; window occlusion callbacks needed | **Primary** |
| **B. Desktop Picture API + Rapid Update** | Use NSWorkspace to change desktop picture repeatedly | Official API | Not designed for video; latency unacceptable (~500ms+); flicker | Not viable |
| **C. ScreenSaver as Wallpaper** | Package as screensaver, run in "preview" mode | Built-in video support | Not a wallpaper; user confusion; doesn't work with apps open | Not viable |
| **D. Quartz Compositor Injection** | Low-level compositor manipulation | Maximum control | Private API, will break, App Store rejection, SIP issues | Not viable |

**Recommended Path (macOS)**: Strategy A (desktop-level NSWindow) as primary. Accept that macOS will be the most constrained platform; document limitations clearly.

**macOS-Specific Pitfalls**:
- **Spaces/Mission Control**: window may not persist across space switches without `NSWindowCollectionBehaviorCanJoinAllSpaces`
- **Notarization**: required for distribution outside App Store; camera access requires entitlement + user permission prompt
- **App Nap**: macOS may throttle background processing; must set `NSAppSleepDisabled` or use `ProcessInfo.processInfo.beginActivity()`
- **Full-screen apps**: macOS creates separate space; wallpaper window invisible (acceptable behavior)
- **Camera permissions**: prompted by OS on first use; app should show pre-prompt explanation
- **macOS Ventura+ changes**: window level behaviors changed; test thoroughly

---

### Linux (X11)

| Strategy | Description | Pros | Cons | Recommendation |
|----------|-------------|------|------|----------------|
| **A. Root Window Drawing** | Draw directly to X11 root window or use _XROOTPMAP_ID | Works on most X11 setups, simple concept, low latency | Compositor may paint over it; some DEs override root | **Primary for X11** |
| **B. _NET_WM_WINDOW_TYPE_DESKTOP** | Create window with EWMH desktop type hint | Standard hint, compositors should respect | Not all WMs honor it; stacking order varies | **Alternative** |
| **C. Compositor-Specific Plugin** | Write plugin for each compositor (KWin, Mutter, etc.) | Native integration | N different implementations, maintenance burden | Future consideration |
| **D. Virtual Monitor / Fake Display** | Create virtual X screen, composite it | Isolation | Extreme complexity, latency | Not viable |

**Recommended Path (Linux X11)**:
- Primary: Strategy A (root window drawing)
- Fallback: Strategy B (EWMH _NET_WM_WINDOW_TYPE_DESKTOP)
- Test on GNOME (Mutter), KDE (KWin), XFCE (Xfwm) compositors

**Linux X11-Specific Pitfalls**:
- DE diversity: GNOME, KDE, XFCE, etc. all behave differently
- Permissions: video device access (`/dev/video*`) may require user in `video` group
- Screensaver: varies by DE; may need to inhibit via D-Bus

---

### Linux (Wayland) — Future Scope

| Strategy | Compositor Support | Notes |
|----------|-------------------|-------|
| **wlr-layer-shell protocol** | wlroots-based (Sway, Hyprland, etc.) | Allows background layer; proper Wayland support |
| **KDE Plasma wallpaper plugin** | KDE Plasma | Has own wallpaper plugin system |
| **GNOME extension** | GNOME | Requires JS extension + native backend; GNOME actively resists live wallpaper |
| **ext-layer-shell (proposed)** | Future compositors | Emerging standard; not widely adopted yet |

**Recommended Path**: Defer Wayland to post-v1. X11 still dominant for power users who would use this tool. Document GNOME+Wayland as "limited support."

---

## 7. Performance & Latency Plan

### Latency Budget Breakdown

```
Target Total: ≤100ms p95 (capture to visible on desktop)

┌────────────────────────────────────────────────────────────────────────────┐
│ Stage                      │ Target   │ Max      │ Notes                   │
├────────────────────────────┼──────────┼──────────┼─────────────────────────┤
│ Camera capture + USB       │ 25-35ms  │ 40ms     │ 30fps = 33ms frame time │
│ OS video subsystem buffer  │ 5-10ms   │ 20ms     │ Request min buffer count│
│ Decode (if MJPEG/H.264)    │ 3-8ms    │ 15ms     │ Hardware decode prefer  │
│ Frame processing           │ 2-5ms    │ 10ms     │ GPU-accelerated path    │
│ Render to desktop surface  │ 5-10ms   │ 20ms     │ Direct render, no copy  │
│ Compositor present         │ 8-16ms   │ 30ms     │ VSync dependent         │
├────────────────────────────┼──────────┼──────────┼─────────────────────────┤
│ TOTAL                      │ 48-84ms  │ 135ms    │ Target p50≤80ms p95≤100 │
└────────────────────────────────────────────────────────────────────────────┘
```

### Latency Reduction Strategies

1. **Minimize buffering**: Request single-frame buffer from camera driver where possible; avoid frame queues longer than 2 frames
2. **Zero-copy paths**: Use GPU texture sharing between decode and render stages; avoid CPU round-trips
3. **Hardware decode**: Prefer hardware MJPEG/H.264 decode over software (OS-provided decoders)
4. **Direct render**: Render to wallpaper surface directly rather than intermediate buffers
5. **Vsync consideration**: May need to disable vsync on wallpaper window for lowest latency (accept tearing as tradeoff; document option)
6. **Frame dropping policy**: If processing can't keep up, **drop oldest frames** rather than queue them (never accumulate latency)

### Frame Rate Stabilization

- Target 30 fps; accept graceful degradation to 15 fps under load
- Implement frame pacing: if camera delivers variable rate, interpolate timing to smooth output
- Monitor frame delivery rate; warn user if camera consistently underdelivers
- **Jank detection**: Track frame timing histogram; flag gaps >2x frame time as jank events

### Resource Management Principles

| Resource | Principle | Target | Action if Exceeded |
|----------|-----------|--------|-------------------|
| **CPU** | Offload to GPU; processing should barely register | ≤10% single core | Reduce fps, simplify overlays |
| **GPU** | Use but don't monopolize; yield to foreground apps | ≤15% utilization | Reduce resolution |
| **Memory** | Fixed buffer pool; no unbounded growth | ≤200 MB, ~3-5 frames in flight | Reduce buffer count |
| **Thermals** | Monitor sustained load | No thermal throttling | Engage eco mode (15 fps) |
| **Power** | Laptop-aware | Eco mode default on battery | 15 fps, reduced processing |

### Performance Measurement Approach

1. **Latency test rig**: High-speed camera filming screen while millisecond timer displayed under microscope; measure photon-to-photon delay
2. **Internal instrumentation**: Timestamp each frame at capture, decode, process, and render stages; compute per-stage deltas
3. **Frame timing histogram**: Track frame delivery intervals; identify jank (gaps >2x expected frame time)
4. **Resource monitoring**: Log CPU/GPU/memory at 1-second intervals during soak tests; compute percentiles

### Performance Acceptance Criteria

- [ ] p50 latency ≤80ms, p95 ≤100ms over 1-hour test
- [ ] ≤5% dropped frames during 30-minute stable test
- [ ] CPU usage ≤10% on reference hardware (mid-range 2020 laptop)
- [ ] Memory stable (±10 MB) over 8-hour test
- [ ] Zero jank events (>2x frame time gaps) during 10-minute stable test

---

## 8. Reliability & Failure Modes

### Failure Mode Enumeration

| Failure Mode | Likelihood | Impact | Detection | Recovery | User Experience |
|--------------|-----------|--------|-----------|----------|-----------------|
| **Camera physically disconnected** | High | Feed stops | USB device removal event | Auto-retry with exponential backoff, then fallback | "Reconnecting..." overlay → fallback + notification |
| **Camera driver reset** | Medium | Temporary loss | Capture error / timeout | Automatic reconnect, seamless if <2s | Brief freeze; auto-recovery |
| **System sleep/wake** | High | Context invalidated | OS power events | Suspend on sleep; reinit on wake | Seamless resume within 5s |
| **Display configuration change** | Medium | Misaligned render | Display change events | Recalculate layout, re-render | Brief flash; auto-adapt |
| **Application crash** | Low | Feed stops | "Running" state file on disk | On restart: detect unclean shutdown, restore wallpaper | Wallpaper stuck until relaunch; offer restore on next start |
| **OS permission revoked (macOS)** | Low | Camera denied | Permission error code | Notify user, guide to System Preferences | Dialog with instructions |
| **GPU driver crash** | Very Low | Render failure | GPU error codes | Fallback to CPU rendering or exit gracefully | Notification; manual restart may be needed |
| **Out of memory** | Very Low | Process killed | N/A | Fixed buffer pool prevents this | Should never occur |
| **Thermal throttling** | Medium | Performance drop | Thermal state APIs | Engage eco mode automatically | Smooth degradation; optional notification |

### Recovery Behavior Flows

```
CAMERA_DISCONNECT:
  1. Immediately show last good frame (freeze) + "Reconnecting..." overlay
  2. Retry connection with exponential backoff:
     - Attempt 1: immediate
     - Attempt 2: 2 seconds
     - Attempt 3: 4 seconds
     - Attempt 4: 8 seconds
     - Attempt 5: 16 seconds
  3. If device returns during retries:
     - Seamlessly resume feed
     - Remove overlay
     - Log event: "Camera reconnected after Xs"
  4. If all retries fail (32+ seconds elapsed):
     - Switch to fallback wallpaper (user's original)
     - Show tray notification: "Camera disconnected. Click to reconnect."
     - Continue polling every 30 seconds in background
  5. If device returns later:
     - Show tray notification: "Camera available. Resume feed?"
     - User can click to resume, or it auto-resumes if configured

SLEEP_WAKE:
  1. On sleep event:
     - Release camera handle
     - Note "sleeping" state
     - Optionally save current frame as freeze image
  2. On wake event:
     - Wait 2-3 seconds (USB enumeration time)
     - Attempt to reopen same camera by device ID
  3. If success:
     - Resume feed seamlessly
     - Log event: "Wake recovery successful"
  4. If fail:
     - Treat as CAMERA_DISCONNECT (start retry sequence)

APPLICATION_CRASH (detected on next startup):
  1. Check "running" state file:
     - If exists and app wasn't running → unclean shutdown detected
  2. Restore user's original wallpaper (stored path in config)
  3. Clear "running" state
  4. Log crash occurrence
  5. Show dialog: "Micround wasn't shut down properly. Your wallpaper has been restored.
     Would you like to restart the live feed?"
  6. User can click "Yes" to resume or "No" to stay on static wallpaper
```

### Safe Defaults

- **Fallback wallpaper**: Store user's original wallpaper path before first activation; restore on any unrecoverable failure
- **Startup behavior**: Default to NOT auto-start; require explicit opt-in
- **Unclean shutdown detection**: Write "running" state to disk on start; clear on clean exit; check on next launch
- **Camera ambiguity**: If multiple cameras present, prompt user to select; do not auto-select
- **Screensaver**: Pause capture during screensaver (configurable)

---

## 9. Security & Privacy

### Threat Model

| Threat | Scope | Likelihood | Severity | Mitigation |
|--------|-------|------------|----------|------------|
| **Accidental recording of sensitive content** | User places document under microscope, forgets | Medium | Medium | No recording by default; no network transmission; clear "what you see is only on your screen" messaging |
| **Camera feed visible to shoulder-surfers** | Physical security | Medium | Low | User's choice; no different from normal desktop visibility |
| **Malicious camera access by other apps** | Out of scope | - | - | Not our problem; standard OS permission model applies |
| **Data exfiltration** | Could be concern if network features existed | Very Low | High | v1 has zero network access; enforce at code/audit level |
| **Privilege escalation** | App runs with user privileges | Very Low | High | No elevated permissions required; don't request admin |
| **Log file sensitive data** | Debug logs might capture frame content | Low | Medium | Never log frame data; only metadata (resolution, fps, errors) |
| **Frame data persisted unintentionally** | Temp files not cleaned | Low | Medium | No disk writes in hot path; temp files cleaned on exit |
| **App used to surveil without user knowledge** | Covert operation | Low | High | Tray icon always visible; "Live" indicator; no hidden mode |

### Local-Only Guarantee

- **No network activity**: App makes zero network connections (verifiable via firewall/audit)
- **No telemetry**: Zero usage tracking, analytics, or crash reporting over network
- **No cloud features**: No sync, sharing, or remote access
- **No auto-update**: User must manually download new versions (or use OS package manager)

### Permissions Model

| OS | Required Permissions | User Experience |
|----|---------------------|-----------------|
| **Windows** | None special (camera access implicit for Win32 apps) | Seamless |
| **macOS** | Camera access (TCC) | System prompt on first use; app shows pre-prompt explanation |
| **Linux** | Video device access (`/dev/video*`) | User must be in `video` group; installer checks and guides |

### Data Handling Principles

1. **No frame storage**: Frames exist only in memory; never written to disk except explicit user snapshot
2. **Snapshots**: Written only to user-specified folder; user initiates each snapshot; includes metadata (timestamp, camera, settings)
3. **Settings file**: Human-readable (JSON/TOML); contains no image data; only settings (camera ID, scale mode, etc.)
4. **Original wallpaper**: Stored as path reference only, not the image data itself
5. **Logs**: Rotated automatically; capped at 10 MB total; contain no image data; deletable by user; retention configurable

### Transparency UX

- **First-run dialog**: "This app displays your camera feed on your desktop. Nothing is recorded or sent anywhere. [Got it]"
- **Tray icon states**:
  - 🟢 "Live" (green) = actively capturing and displaying
  - ⏸️ "Paused" = frozen frame
  - ⚪ "Stopped" = not capturing
- **Panic stop hotkey**: Instantly restores original wallpaper and pauses capture
- **Optional on-desktop status**: Small “Live” watermark toggle for extra visibility
- **"Recording: Off" indicator**: Visible in settings to reassure users
- **Settings → Privacy section**: Clear explanation of data handling
- **About → Privacy**: Link to privacy policy document

---

## 10. UX / Product Design

### Minimal Control Surface

```
SYSTEM TRAY ICON
├── [Status line: "🟢 Live: 1920x1080 @ 30fps" or "⏸️ Paused" or "⚠️ No camera"]
├── ─────────────────────────
├── ▶ Start / ⏹ Stop Feed (toggle based on state)
├── ⏸ Pause / ▶ Resume (freeze frame toggle)
├── 📸 Take Snapshot (→ clipboard + file)
├── 🛑 Privacy Stop (restore wallpaper immediately)
├── ─────────────────────────
├── 📷 Camera: [Current Camera Name] ▶ (submenu: camera list)
├── 🖥 Display: [Current Target] ▶ (submenu: monitor list + "All")
├── ⚖️ Scaling: [Current Mode] ▶ (submenu: Fit/Fill/Stretch/Center)
├── 🔄 Rotation: [Current] ▶ (submenu: 0°/90°/180°/270°)
├── ↔️ Flip: ▶ (submenu: None/Horizontal/Vertical/Both)
├── ─────────────────────────
├── 📋 Presets ▶ (submenu: saved presets + "Save Current...")
├── 🪟 Show Preview Window (toggle floating preview)
├── ⚙️ Settings...
├── ─────────────────────────
└── ✖ Quit

SETTINGS WINDOW (tabbed or single scrollable page)
├── 📷 Camera
│   ├── Device dropdown (auto-populated, refresh button)
│   ├── Resolution dropdown (camera-supported options)
│   ├── Frame rate dropdown (15/24/30/60 fps where supported)
│   └── [Live preview thumbnail]
├── 🖥 Display
│   ├── Target monitor dropdown (list + "All Monitors")
│   ├── Scaling mode: Fit / Fill / Stretch / Center
│   ├── Rotation: 0° / 90° / 180° / 270°
│   ├── Flip: None / Horizontal / Vertical / Both
│   └── [Smart auto-crop: "Best View" button]
├── 🎨 Overlay (collapsed by default)
│   ├── Show timestamp checkbox + format dropdown
│   ├── Show scale bar checkbox + calibration helper
│   ├── Show magnification label (user-set)
│   ├── Custom text field
│   ├── Show "Live" indicator checkbox
│   ├── Auto-hide overlay on mouse move checkbox
│   └── Position: Corner selector (9-position grid)
├── ⚡ Performance
│   ├── Quality mode: Auto (Latency / Quality / Battery) / Locked
│   ├── Hardware acceleration: On / Off / Auto
│   └── [Current stats: CPU X%, GPU Y%, latency Zms]
├── 🚀 Startup
│   ├── Launch at login checkbox
│   ├── Start feed automatically checkbox
│   └── Start minimized checkbox
├── 🔒 Privacy
│   ├── [Info: "No data is recorded or sent anywhere"]
│   ├── Snapshot folder: [path] [Browse]
│   ├── Log level: Normal / Verbose / Debug
│   └── [Clear logs button]
├── 📋 Presets
│   ├── [List of saved presets with Load/Delete buttons]
│   └── [Save Current Settings button]
└── ⚙️ Advanced
    ├── Fallback wallpaper: Use original / Solid color picker
    ├── Reconnection: Auto-reconnect enabled checkbox
    ├── Vsync: On / Off (for lowest latency)
    └── [Reset all settings button]
```

### Multi-Monitor Behavior Rules

| Scenario | Default Behavior | User Override Available |
|----------|------------------|------------------------|
| Single monitor | Feed fills that monitor | N/A |
| Multi-monitor, one selected | Feed on selected; others keep existing wallpaper | Can select any monitor |
| Multi-monitor, "All" selected | Same feed on all (scaled independently per resolution/DPI) | N/A |
| Mixed DPI | Scale to target display DPI automatically | N/A (automatic) |
| Monitor added while running | Prompt: "New display detected. Show feed here too? [Yes] [No]" | User choice |
| Selected monitor removed | Move feed to primary monitor; show notification | Automatic |
| Display resolution change | Re-scale feed; no user action needed | Automatic |

### Accessibility Considerations

| Concern | Implementation |
|---------|----------------|
| **Screen reader** | Tray icon and all menu items have proper accessibility labels |
| **Keyboard navigation** | All settings accessible via Tab/Enter; tray menu via keyboard shortcuts |
| **High contrast** | Overlay text has shadow/outline for visibility on any background; respects OS high-contrast mode |
| **Motion sensitivity** | "Eco mode" option reduces frame rate to 15fps; pause function prominent |
| **Color blindness** | Status indicators use text + icon shape, not color alone (🟢 "Live", ⏸️ "Paused", ⚠️ "Error") |
| **Large text** | Settings window respects OS font scaling |

---

## 11. Validation & Test Plan

### Test Matrix

| Dimension | Values to Test |
|-----------|----------------|
| **OS** | Windows 10 (21H2, 22H2), Windows 11 (22H2, 23H2, 24H2), macOS 12/13/14/15, Ubuntu 22.04/24.04, Fedora 39/40 |
| **Desktop Environment (Linux)** | GNOME (X11), GNOME (Wayland - limited), KDE Plasma (X11), KDE Plasma (Wayland), XFCE |
| **Camera Resolution** | 640x480, 1280x720, 1920x1080, 3840x2160 (4K) |
| **Camera Format** | MJPEG, YUY2, H.264 (if supported) |
| **Camera Hardware** | 3+ USB microscopes (different vendors), 2+ HDMI capture devices, built-in webcam (sanity) |
| **Monitor Count** | 1, 2, 3 |
| **Monitor Configuration** | Same resolution, mixed resolution, mixed DPI, portrait + landscape |

### Test Scenarios

| ID | Scenario | Pass Criteria |
|----|----------|---------------|
| **T01** | First launch, single camera | Camera auto-detected; preview shown; one-click to wallpaper |
| **T02** | First launch, multiple cameras | Camera selection shown; user picks one; preview updates |
| **T03** | Camera disconnect during feed | Fallback within 2s; "Reconnecting..." overlay; notification |
| **T04** | Camera reconnect after disconnect | Auto-resumes within 3s; overlay removed; seamless |
| **T05** | Camera remains disconnected | Fallback wallpaper after 32s; notification; background polling |
| **T06** | Sleep/wake cycle | Feed resumes within 5s of wake; no user action |
| **T07** | Display resolution change | Wallpaper re-scales immediately; no user action |
| **T08** | Add external monitor | Prompt appears; user can choose to extend feed |
| **T09** | Remove active monitor | Feed moves to remaining display; notification |
| **T10** | Full-screen app launched | No interference; feed continues updating (invisible) |
| **T11** | Snapshot capture | Image saved to correct location; clipboard populated; feed uninterrupted |
| **T12** | Settings change (scale mode) | Applied immediately; no restart required |
| **T13** | Rotation change | Applied immediately; correct orientation |
| **T14** | Pause/resume | Pause freezes frame; resume continues from live |
| **T15** | App quit | Original wallpaper restored within 1s |
| **T16** | App crash (simulated kill -9) | On relaunch: crash detected, wallpaper restored, prompt shown |
| **T17** | Preset save/load | Settings saved correctly; load applies all settings |
| **T18** | Eco mode activation | Frame rate drops to 15fps; CPU usage decreases |
| **T19** | Screensaver activation | Feed pauses (configurable); resumes after |
| **T20** | Permission denied (macOS) | Clear error message; link to System Preferences |

### Long-Run Soak Testing

| Test | Duration | Success Criteria |
|------|----------|------------------|
| **Continuous operation (idle desktop)** | 24 hours | No crash, no memory growth (±10 MB), fps stable (≥24), p95 latency ≤100ms |
| **Continuous operation (active desktop use)** | 8 hours | No crash, normal work unimpeded, ≤5% dropped frames |
| **Sleep/wake cycles** | 8 hours (wake every 30 min = 16 cycles) | 100% successful recovery |
| **Camera disconnect/reconnect cycles** | 8 hours (disconnect every hour = 8 cycles) | 100% successful reconnect |
| **Display changes** | 4 hours (add/remove monitor every 30 min) | No crash, correct re-layout every time |
| **Thermal stress** | 2 hours (high CPU load in background) | Eco mode engages, no thermal throttling of OS |

### Acceptance Criteria Checklist

**Functional**
- [ ] Camera auto-detection works on all OS targets
- [ ] Live feed displays as wallpaper within 5 seconds of "Start"
- [ ] All scaling modes (Fit/Fill/Stretch/Center) produce correct output
- [ ] Rotation (0°/90°/180°/270°) works correctly
- [ ] Flip (H/V/Both) works correctly
- [ ] Pause freezes frame; resume continues
- [ ] Snapshot saves to expected location and clipboard
- [ ] Settings persist across restart
- [ ] Quit restores original wallpaper
- [ ] Startup at login works when enabled
- [ ] Presets save and load correctly
- [ ] Multi-monitor selection works correctly

**Non-Functional**
- [ ] Latency ≤100ms p95 (measured)
- [ ] Frame rate ≥24 fps sustained (measured)
- [ ] CPU ≤10% single core (measured)
- [ ] GPU ≤15% (measured)
- [ ] Memory stable (±10 MB) over 8 hours
- [ ] No crashes in 24-hour soak
- [ ] Sleep/wake recovery 100% success rate
- [ ] Camera reconnection 100% success rate

**Compatibility**
- [ ] Works on all OS versions in test matrix
- [ ] Works with all cameras in test matrix
- [ ] Works on all monitor configurations in test matrix
- [ ] Works with all Linux DEs in test matrix (X11)

**Security/Privacy**
- [ ] Zero network connections (verified with firewall/packet capture)
- [ ] No frame data in log files
- [ ] Uninstall leaves no orphaned files

---

## 12. Milestones & Deliverables

### Phase Overview

| Phase | Goal | Key Deliverable | Exit Criteria |
|-------|------|-----------------|---------------|
| **Prototype** | Prove core technical feasibility | Demo: camera → wallpaper on Windows | ≤100ms latency, ≥24 fps demonstrated |
| **Alpha** | Feature-complete on primary OS | Windows app with full UI + installer | All "Must" requirements working; 8-hour soak pass |
| **Beta** | Cross-platform + stability | Win/Mac/Linux builds; beta testers | 24-hour soak pass on all platforms; recovery logic tested |
| **v1.0** | Public release ready | Signed builds, docs, support channel | All acceptance criteria met |

### Phase Details

#### Prototype Phase
**Proves**: The core technical approach works

**Deliverables**:
- Single OS (Windows recommended)
- Hardcoded camera (first detected)
- Basic scaling (fill mode only)
- No UI (command-line or code-level config)
- Latency measurement instrumentation

**Success Criteria**:
- ≤100ms p95 latency demonstrated
- ≥24 fps sustained for 30 minutes
- WorkerW injection works on Windows 10 + 11

**Risks & Mitigations**:
| Risk | Likelihood | Mitigation |
|------|------------|------------|
| WorkerW approach doesn't work on modern Windows | Medium | Test on Windows 11 first; have DirectComposition backup ready |
| Camera latency too high | Low | Test with known low-latency camera; adjust target if needed |
| Latency budget fundamentally unachievable | Low | Accept higher target (≤150ms) or reduce fps target |

---

#### Alpha Phase
**Proves**: The product is usable by target users

**Deliverables**:
- Full settings UI (tray menu + settings window)
- All scaling/rotation/flip modes
- Pause/resume, snapshot
- Settings persistence
- Camera selection (multiple cameras)
- Single-monitor support
- Session presets (basic)
- Installer (MSI or equivalent)

**Success Criteria**:
- All "Must" functional requirements working
- 8-hour soak test passes
- 3+ camera models tested

**Risks & Mitigations**:
| Risk | Likelihood | Mitigation |
|------|------------|------------|
| UI framework impacts performance | Medium | Benchmark UI; keep settings window closed during feed |
| Settings permutations have bugs | Medium | Thorough QA matrix; automated tests for persistence |
| Camera compatibility issues | Medium | Test 5+ camera models; document compatibility list |

---

#### Beta Phase
**Proves**: The product is stable across platforms

**Deliverables**:
- macOS support (desktop-level window)
- Linux X11 support (root window drawing)
- Multi-monitor support
- Sleep/wake recovery
- Camera reconnection (auto-recovery engine)
- Full preset system
- Eco mode / adaptive quality
- 24-hour soak tests pass
- Installer/packaging for all platforms (DMG, AppImage/deb/rpm)
- Beta distribution channel
- Feedback collection mechanism

**Success Criteria**:
- All functional requirements working on all platforms
- 24-hour soak test passes on all platforms
- 50+ beta testers with diverse hardware

**Risks & Mitigations**:
| Risk | Likelihood | Mitigation |
|------|------------|------------|
| macOS notarization issues | Medium | Start notarization process early; test on hardware |
| macOS App Nap throttles feed | Medium | Implement beginActivity(); test explicitly |
| Linux DE fragmentation | High | Limit official support to GNOME/KDE/XFCE on X11; document others as "community" |
| Beta feedback reveals major UX issues | Medium | Plan buffer time for iteration |

---

#### v1.0 Release Phase
**Proves**: Production-ready quality

**Deliverables**:
- Code-signed builds (Windows, macOS notarized)
- User documentation (quick start guide, FAQ, troubleshooting)
- Known issues and camera compatibility list
- Support channel (GitHub Issues or similar)
- Landing page / distribution site
- All acceptance criteria passed

**Success Criteria**:
- All acceptance criteria checklist items checked
- Zero critical/blocker bugs
- Documentation complete

**Risks & Mitigations**:
| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Last-minute blocker on one platform | Medium | Allow v1.0 to ship without one platform if necessary; follow up in v1.1 |
| Performance regressions in final build | Low | Automated performance tests in CI |
| Distribution/signing issues | Low | Test full distribution flow before launch |

---

## 13. Nice-to-Haves / Future Backlog

### Prioritized Backlog

| Priority | Feature | Description | Complexity | Value |
|----------|---------|-------------|------------|-------|
| **1** | **Video recording / timelapse** | Record feed to file; timelapse mode (1 frame/N seconds) | Medium | High |
| **2** | **Wayland support** | wlr-layer-shell for wlroots compositors; KDE plugin | High | High |
| **3** | **Zoom/pan** | Digital zoom of camera feed; pan within zoomed view | Medium | Medium |
| **4** | **Multiple camera sources** | Picture-in-picture or split view | High | Medium |
| **5** | **Precision measurement tools** | Calibrated rulers, area/length readouts, advanced scale bar | Medium | Medium |
| **6** | **Color calibration** | White balance, exposure, color temperature adjustment | Medium | Medium |
| **7** | **Focus stacking preview** | Capture multiple focal planes, composite (needs motorized focus) | High | Low |
| **8** | **ML-based detection** | Cell counting, motion detection, auto-focus hunting alerts | High | Low |
| **9** | **Remote viewing** | Secure local network streaming to phone/tablet | High | Medium |
| **10** | **Annotation tools** | Draw on feed, arrow pointers, text labels | Medium | Low |
| **11** | **Microscope metadata integration** | Read magnification, stage position from supported scopes | High | Low |
| **12** | **Plugin system** | Third-party overlays and image processing | High | Low |

### High-Impact Feature Ideas (Requiring Research)

| Feature | Use Case | Research Needed |
|---------|----------|-----------------|
| **Audio reactivity** | Art installation: modulate visual parameters based on audio input | Audio analysis approach; performance impact |
| **VR/AR integration** | Feed into VR environment as texture | Platform APIs; latency requirements |
| **Collaborative viewing** | Multiple users see same feed with shared annotations | Networking architecture; sync protocol |
| **Raspberry Pi headless** | Run capture on Pi, stream to desktop | Streaming protocol; Pi camera support |
| **Web-based control** | Browser UI for settings (local only) | Local web server security; resource overhead |
| **Lab software integration** | Export to Electronic Lab Notebook (ELN) systems | ELN APIs; metadata formats |

---

## 14. Summary of Recommendations

### Recommended Technical Path

1. **Start with Windows** using WorkerW window-behind-icons approach (DirectComposition as fallback)
2. **Target UVC cameras exclusively** for v1; covers vast majority of microscope cameras
3. **Build minimal UI** (tray menu + single settings window); avoid feature creep
4. **Prioritize latency (≤100ms) and stability (72+ hours)** over features
5. **Implement adaptive quality** early; ensure good experience on modest hardware
6. **Add macOS second** using NSWindow at desktop level (handle App Nap, notarization)
7. **Add Linux X11 third** using root window drawing (test on GNOME, KDE, XFCE)
8. **Defer Wayland, recording, and network features** to post-v1

### Key Success Metrics (v1.0)

| Metric | Target | Stretch |
|--------|--------|---------|
| Time from install to live wallpaper | ≤2 minutes | ≤1 minute |
| End-to-end latency (p95) | ≤100ms | ≤80ms |
| Sustained uptime without intervention | ≥72 hours | ≥168 hours (1 week) |
| User-reported "it just works" sentiment | ≥80% | ≥90% |
| Crash rate | ≤1 per 100 hours | ≤1 per 500 hours |
| Sleep/wake recovery success | 100% | 100% |
| Camera reconnection success | ≥99% | 100% |

### Critical Path Items

1. **Validate Windows wallpaper injection** on Windows 10 21H2/22H2 and Windows 11 22H2/23H2/24H2
2. **Validate macOS desktop-level window** permissions without App Store distribution
3. **Confirm ≤100ms latency achievable** with representative USB microscope cameras
4. **Build auto-recovery engine early** (most common user pain point is disconnection)
5. **Implement adaptive quality** to ensure good experience on laptops and modest desktops

---

## 15. Open Questions for Stakeholder Clarification

The following questions should be answered before or during the Prototype phase to inform scope and priority decisions:

| # | Question | Impact |
|---|----------|--------|
| 1 | **Primary target user**: Is this for hobbyists/educators or laboratory/professional use? | Affects polish level, installer requirements, support expectations |
| 2 | **Distribution model**: Open source? Paid? Freemium? | Affects build/release pipeline, monetization, support model |
| 3 | **Reference hardware**: What's the minimum spec machine we should target? | Affects performance budget, adaptive quality thresholds |
| 4 | **Camera compatibility**: Any specific microscope cameras we must support? | May surface non-UVC requirements early |
| 5 | **Latency tolerance**: Is ≤100ms truly required, or is ≤150ms acceptable? | Affects architecture decisions, vsync policy |
| 6 | **Linux priority**: Is Linux X11 support required for v1, or can it slip to v1.1? | Affects timeline, testing burden |
| 7 | **Multi-monitor priority**: Is multi-monitor required for v1, or single-monitor sufficient? | Affects timeline, complexity |
| 8 | **Auto-update mechanism**: Should v1 have auto-update, or manual download only? | Affects privacy stance, infrastructure needs |

---

*End of Project Plan*
