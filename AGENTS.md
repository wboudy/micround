## Session Start (Required First Step)

At the start of **EVERY** coding session, run:
```bash
source scripts/agent-env.sh
```

This activates CI-first enforcement tools. You will see a confirmation message.
If you skip this step, cargo-guard will not block local builds.

**Automatic Activation**: If your environment pre-sets `CLAUDE_AGENT=true`, or if you
use direnv with the `.envrc` file, activation is automatic. Verify with:
```bash
echo $CLAUDE_AGENT    # Should print "true"
cargo build           # Should be BLOCKED
```

---

## CRITICAL: CI-First Build/Test Policy

**All Rust compilation MUST happen in GitHub Actions CI, NOT locally.**

This server runs multiple agents concurrently. Local cargo builds would:
- Consume 4-8GB RAM each, starving other agents
- Spike CPU usage across all cores
- Make cross-platform testing impossible (we need Linux, Windows, macOS)

### Forbidden Local Commands

**NEVER run these commands locally:**
```
cargo build (any variant)
cargo test (any variant)
cargo check
cargo clippy
cargo run
cargo bench
```

If you try, `cargo-guard` will block the command with an error message.

### Allowed Local Commands

These operations involve NO compilation and ARE allowed:
```bash
cargo fmt --check    # Check formatting (zero compilation)
cargo fmt            # Auto-format code (zero compilation)
cargo --version      # Version info
cargo metadata       # Project metadata
ci fmt               # Format + stage changes (convenience)
```

---

## CI Workflow Checklist

Follow this workflow for EVERY code change:

### 1. Create Feature Branch
```bash
git checkout -b feature/bd-xyz-description
```

### 2. Make Changes (NO local builds!)
- Edit source files
- Run `ci fmt` to format code
- Commit incrementally

### 3. Submit to CI
```bash
git add <files>
git commit -m "feat: implement feature (bd-xyz)"
ci submit    # Creates PR, pushes, waits for CI
```

### 4. Handle CI Failures
```bash
ci logs              # View failure details
# ... fix issues ...
git add <files>
git commit -m "fix: address CI failures"
git push
ci wait              # Wait for CI again
```

### 5. Merge When Green
```bash
ci merge             # Squash merge + delete branch
```

### 6. Clean Up
```bash
git checkout main && git pull
bd close bd-xyz
bd sync
```

---

## CI Checks Reference

| Check | What It Validates |
|-------|-------------------|
| `lint` | `cargo fmt --check` + `cargo clippy -D warnings` |
| `test-linux` | Build + tests with `--features linux` |
| `test-windows` | Build + tests with `--features windows` |
| `test-macos` | Build + tests with `--features macos` |
| `coverage` | Code coverage via tarpaulin |
| `docs` | rustdoc generation |
| `ci-success` | All above checks passed |

---

## Quick Reference

| Task | Command |
|------|---------|
| Format code | `ci fmt` |
| Submit PR | `ci submit` |
| Check CI status | `ci status` |
| Wait for CI | `ci wait` |
| View CI logs | `ci logs` |
| Merge PR | `ci merge` |
| Create PR manually | `gh pr create` |
| View PR checks | `gh pr checks` |
| Re-run failed jobs | `gh run rerun <run-id> --failed` |

---

## CI Debugging

### Common CI-Only Failures

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| Windows build fails | Missing `#[cfg(windows)]` guard | Add platform-specific cfg |
| macOS build fails | Missing objc2 dependency | Check Cargo.toml features |
| Clippy warnings | Unused imports, dead code | Fix warnings or add allows |
| Format check fails | Forgot `ci fmt` | Run `ci fmt` and push |

### Useful Commands

```bash
# Download CI artifacts
gh run download <run-id>

# View specific job logs
gh run view <run-id> --log-failed

# Re-run only failed jobs
gh run rerun <run-id> --failed

# List recent workflow runs
gh run list --limit 5
```

---

## Git Workflow

- NEVER commit directly to main
- Create feature branches: `git checkout -b feature/description`
- Make commits on feature branch
- Create PR when ready: `gh pr create`

<!-- bv-agent-instructions-v1 -->

---

## Beads Workflow Integration

This project uses [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) for issue tracking. Issues are stored in `.beads/` and tracked in git.

### Essential Commands

```bash
# View issues (launches TUI - avoid in automated sessions)
bv

# CLI commands for agents (use these instead)
bd ready              # Show issues ready to work (no blockers)
bd list --status=open # All open issues
bd show <id>          # Full issue details with dependencies
bd create --title="..." --type=task --priority=2
bd update <id> --status=in_progress
bd close <id> --reason="Completed"
bd close <id1> <id2>  # Close multiple issues at once
bd sync               # Commit and push changes
```

### Workflow Pattern

1. **Start**: Run `bd ready` to find actionable work
2. **Claim**: Use `bd update <id> --status=in_progress`
3. **Work**: Implement the task
4. **Complete**: Use `bd close <id>`
5. **Sync**: Always run `bd sync` at session end

### Key Concepts

- **Dependencies**: Issues can block other issues. `bd ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog (use numbers, not words)
- **Types**: task, bug, feature, epic, question, docs
- **Blocking**: `bd dep add <issue> <depends-on>` to add dependencies

### Session Protocol

**Before ending any session, run this checklist:**

```bash
git status              # Check what changed
git add <files>         # Stage code changes
bd sync                 # Commit beads changes
git commit -m "..."     # Commit code
bd sync                 # Commit any new beads changes
git push                # Push to remote
```

### Best Practices

- Check `bd ready` at session start to find available work
- Update status as you work (in_progress → closed)
- Create new issues with `bd create` when you discover tasks
- Use descriptive titles and set appropriate priority/type
- Always `bd sync` before ending session

<!-- end-bv-agent-instructions -->
