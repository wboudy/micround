# Micround

Micround is a desktop application that displays a live microscope camera feed as your system wallpaper. The goal is a low‑latency, reliable, cross‑platform experience that turns the desktop background into a continuously updating microscopic view.

## Status

- **Active development** (~65% complete)
- Built with **Rust** + wgpu (graphics) + egui (UI)
- Cross-platform: Windows, macOS, Linux

### Platform Support

| Platform | Capture | Rendering | Wallpaper | Status |
|----------|---------|-----------|-----------|--------|
| Windows | Media Foundation | D3D11/wgpu | WorkerW | In progress |
| macOS | AVFoundation | Metal | NSWindow | ✅ Complete |
| Linux | V4L2 | Vulkan/wgpu | X11 | ✅ Complete |

## Repository Structure

```
micround/
├── src/
│   ├── core/          # Platform-independent types, errors, events
│   ├── capture/       # Video capture (V4L2, Media Foundation, AVFoundation)
│   ├── process/       # Frame processing (scaling, transforms)
│   ├── render/        # Wallpaper rendering backends
│   ├── platform/      # Platform abstractions (permissions, autostart)
│   ├── ui/            # System tray, settings
│   ├── config/        # Configuration handling
│   ├── engine.rs      # Display engine orchestration
│   └── snapshot.rs    # Frame capture to file/clipboard
├── tests/             # Unit, integration, E2E, and soak tests
├── scripts/           # CI automation (ci-loop.sh, ci-extract-errors.sh)
├── .claude/skills/    # Claude Code skills (/ship for CI automation)
└── .beads/            # Issue tracking
```

## Key Principles

- **Low latency** (target ≤100ms p95)
- **Privacy-first** (no network, no recording by default)
- **Cross-platform** (Windows, macOS, Linux)
- **Reliability** (auto-recovery from disconnects, sleep/wake, display changes)

## Building

```bash
# Linux
cargo build --features linux

# macOS
cargo build --features macos

# Windows
cargo build --features windows
```

## Testing

```bash
# Run all tests
cargo test --features <platform>

# Run specific test suites
cargo test --features <platform> e2e_      # E2E tests
cargo test --features <platform> perf_     # Performance tests
```

## Documentation

- **[Getting Started](docs/GETTING_STARTED.md)** - Quick installation and setup guide
- **[User Guide](docs/USER_GUIDE.md)** - Complete usage documentation
- **[Troubleshooting](docs/TROUBLESHOOTING.md)** - Common issues and solutions
- **[FAQ](docs/FAQ.md)** - Frequently asked questions

### Technical Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design and component overview
- [Interfaces](docs/INTERFACES.md) - API and integration documentation
- [Privacy](docs/PRIVACY.md) - Privacy policy and data handling

## Issue Tracking

This project uses Beads for tracking tasks in `.beads/`.

```bash
bd ready        # List ready tasks
bd show <id>    # Show task details
bd update <id>  # Update task status
```

## License

MIT
