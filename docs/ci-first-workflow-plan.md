# Plan: CI-First Workflow Enforcement for Micround

## Goal
Enforce a strict CI-first workflow where:
1. **No local heavy builds** - Rust builds/tests must NOT run locally
2. **Every change via PR** - No direct commits to main
3. **Agents wait for CI** - Must block until workflow completes
4. **Fix failures and retry** - Iterate until CI passes
5. **Merge only when green** - Hard block on merge without passing CI

---

## Implementation Overview

### Files to Create
| File | Purpose |
|------|---------|
| `scripts/ci` | Main CLI tool with `submit`, `wait`, `merge`, `status` commands |
| `scripts/cargo-guard` | Wrapper that blocks `cargo build/test` locally |
| `.githooks/pre-commit` | Format check + warn about CI-first |
| `.githooks/pre-merge-commit` | BLOCKS merge unless CI green |
| `scripts/setup-hooks.sh` | Install git hooks |
| `scripts/setup-branch-protection.sh` | Configure GitHub branch protection |

### Files to Modify
| File | Changes |
|------|---------|
| `AGENTS.md` | Add comprehensive CI-first workflow section |
| `docs/TESTING.md` | Add CI-first notice at top |

---

## Part 1: The `ci` CLI Tool

**File: `scripts/ci`**

A single entry point for all CI workflow operations:

```
ci submit     # Push to PR and wait for CI (blocking)
ci status     # Show current CI status
ci wait       # Wait for CI to complete (blocking)
ci merge      # Merge PR (only if CI green)
ci logs       # View failed CI logs
ci fmt        # Run cargo fmt (only allowed local operation)
```

Key behaviors:
- `ci submit` creates PR if needed, pushes, then calls `ci wait`
- `ci wait` polls `gh pr checks` every 30s until completion
- `ci merge` checks CI status first, BLOCKS if not all green
- All blocking operations print clear status messages

---

## Part 2: Cargo Guard (Block Local Builds)

**File: `scripts/cargo-guard`**

A wrapper script that intercepts cargo commands:

**Allowed locally:**
- `cargo fmt` - Fast, zero compilation
- `cargo --version`, `cargo help`, `cargo metadata` - No compilation

**BLOCKED locally (all compilation):**
- `cargo build` - Use CI
- `cargo test` - Use CI
- `cargo run` - Use CI
- `cargo check` - Use CI (still compiles)
- `cargo clippy` - Use CI (compiles + lints)
- `cargo bench` - Use CI

> **Note:** With many agents running on this server, even lightweight
> compilation adds up. Only truly zero-compilation operations are allowed.

When blocked, prints helpful message directing to `ci submit`.

**Activation:** Agents source `scripts/agent-env.sh` which:
- Sets `CLAUDE_AGENT=true`
- Aliases `cargo` to `cargo-guard`

---

## Part 3: Git Hooks

### `.githooks/pre-commit`
- Runs `cargo fmt --check` (zero compilation, allowed)
- Warns about CI-first workflow
- Prints reminder to use `ci submit` after push
- Detects if heavy cargo operations were recently run (warns)

### `.githooks/pre-merge-commit`
- **HARD BLOCK** - Checks `gh pr checks`
- If any check PENDING or FAILED → exit 1, block merge
- Only allows merge when ALL checks SUCCESS

### `scripts/setup-hooks.sh`
- Copies hooks to `.githooks/`
- Runs `git config core.hooksPath .githooks`

---

## Part 4: GitHub Branch Protection

**File: `scripts/setup-branch-protection.sh`**

Configures via `gh api`:
- Require `CI Success` status check to pass
- Require branch up-to-date with main
- Require linear history (squash merges)
- Block force pushes
- Block direct pushes to main

Run once by a human with admin access.

---

## Part 5: Documentation Updates

### AGENTS.md Additions

New section "CI-First Development Workflow" covering:

1. **Forbidden local commands** - List of blocked cargo commands
2. **Required workflow** - Step-by-step from branch to merge
3. **CI commands reference** - All `ci` and `gh` commands
4. **Handling CI failures** - How to read logs, fix, retry
5. **Session end checklist** - Updated to include CI status check

### docs/TESTING.md

Add prominent notice at top:
> **CI-First Workflow**: All tests run in GitHub Actions CI, not locally.

---

## Agent Workflow Example

```bash
# 1. Start work
bd ready
bd update bd-xyz --status=in_progress
git checkout -b feature/bd-xyz-description

# 2. Make changes (NO local builds)
# ... edit files ...
ci fmt                              # Format (allowed)

# 3. Submit to CI
git add src/file.rs
git commit -m "feat: implement feature (bd-xyz)"
ci submit                           # Push + create PR + WAIT for CI

# 4. If CI fails
ci logs                             # View failure details
# ... fix issues ...
git commit -m "fix: address CI failures"
git push
ci wait                             # Wait again

# 5. Merge when green
ci merge                            # Squash merge + delete branch

# 6. Clean up
bd close bd-xyz
bd sync
```

---

## Verification

After implementation, verify:

1. **Cargo guard works:**
   ```bash
   source scripts/agent-env.sh
   cargo build   # Should be BLOCKED
   cargo clippy  # Should be BLOCKED
   cargo fmt     # Should work (only allowed operation)
   ```

2. **Git hooks installed:**
   ```bash
   ./scripts/setup-hooks.sh
   git config core.hooksPath  # Should show .githooks
   ```

3. **CI workflow:**
   ```bash
   git checkout -b test/ci-workflow
   echo "// test" >> src/lib.rs
   git add . && git commit -m "test"
   ci submit  # Should create PR and wait
   ci status  # Should show checks
   ```

4. **Merge blocking:**
   ```bash
   # With failing CI, attempt:
   ci merge  # Should be BLOCKED
   ```

5. **Branch protection (after running setup):**
   ```bash
   git checkout main
   git push  # Should be BLOCKED (no direct push)
   ```

---

## File Structure After Implementation

```
micround/
├── AGENTS.md                    # MODIFIED
├── docs/
│   └── TESTING.md               # MODIFIED
├── scripts/
│   ├── ci                       # NEW - main workflow CLI
│   ├── cargo-guard              # NEW - blocks local builds
│   ├── agent-env.sh             # NEW - agent environment setup
│   ├── setup-hooks.sh           # NEW - install git hooks
│   └── setup-branch-protection.sh  # NEW - GitHub config
└── .githooks/
    ├── pre-commit               # NEW - format check + reminder
    └── pre-merge-commit         # NEW - BLOCKS merge without CI
```

---

## Summary

| Requirement | Implementation |
|-------------|----------------|
| No local builds | `cargo-guard` wrapper + `CLAUDE_AGENT` env |
| Every change via PR | Branch protection + documentation |
| Wait for CI | `ci wait` (blocking poll) |
| Fix and retry | `ci logs` + push + `ci wait` loop |
| Merge only when green | `pre-merge-commit` hook + `ci merge` check |
