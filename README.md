# Micround

Micround is a desktop application that displays a live microscope camera feed as your system wallpaper. The goal is a low‑latency, reliable, cross‑platform experience that turns the desktop background into a continuously updating microscopic view.

## Status
- **Planning/architecture phase** (no implementation yet)
- Default tech stack: **Rust** (decision tracked in bead `bd-3jn`)
- See the comprehensive plan in `docs/PROJECT_PLAN.md`

## Repository Structure (Planned)
```
micround/
├── src/
│   ├── core/          # Platform-independent logic
│   ├── capture/       # Video ingest layer
│   ├── process/       # Frame processing
│   ├── render/        # Wallpaper backends
│   │   ├── windows/
│   │   ├── macos/
│   │   └── linux/
│   ├── ui/            # Control surface
│   └── config/        # Configuration handling
├── tests/
├── docs/
├── assets/
└── scripts/
```

## Key Principles
- **Low latency** (target ≤100ms p95)
- **Privacy-first** (no network, no recording by default)
- **Cross-platform** (Windows, macOS, Linux)
- **Reliability** (auto-recovery from disconnects, sleep/wake, display changes)

## Issue Tracking
This project uses Beads for tracking tasks in `.beads/`.

## License
MIT (default).
