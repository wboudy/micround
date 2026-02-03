#!/bin/bash
# agent-env.sh: CI-first environment activation for agents
#
# Usage: source this file at the start of your session
#   source scripts/agent-env.sh
#
# This sets up the CI-first environment:
# - CLAUDE_AGENT=true (enables cargo-guard blocking)
# - cargo alias points to cargo-guard wrapper
# - PATH includes scripts/ for ci tool
#
# Human developers should NOT source this file.

# Determine script location (works when sourced)
if [ -n "${BASH_SOURCE[0]}" ]; then
  _AGENT_ENV_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
  # Fallback for non-bash shells (zsh, etc.)
  _AGENT_ENV_SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi
_AGENT_ENV_PROJECT_ROOT="$(dirname "$_AGENT_ENV_SCRIPT_DIR")"

# Verify we're in the right project
if [ ! -f "$_AGENT_ENV_PROJECT_ROOT/Cargo.toml" ]; then
  echo "WARNING: Could not find project root"
  echo "Expected Cargo.toml at: $_AGENT_ENV_PROJECT_ROOT/Cargo.toml"
  echo ""
fi

# Check gh CLI availability (warn but don't fail)
if ! command -v gh &>/dev/null; then
  echo "WARNING: gh CLI not found"
  echo "CI workflow commands will fail without it."
  echo ""
  echo "Install from: https://cli.github.com/"
  echo "  macOS:  brew install gh"
  echo "  Linux:  sudo apt install gh"
  echo ""
fi

# Mark this as an agent session
export CLAUDE_AGENT=true

# Alias cargo to the guard wrapper
# shellcheck disable=SC2139
alias cargo="$_AGENT_ENV_SCRIPT_DIR/cargo-guard"

# Add scripts to PATH for ci tool (only if not already there)
case ":$PATH:" in
  *":$_AGENT_ENV_SCRIPT_DIR:"*)
    # Already in PATH
    ;;
  *)
    export PATH="$_AGENT_ENV_SCRIPT_DIR:$PATH"
    ;;
esac

# Confirmation message
cat <<'EOF'
========================================
  CI-first environment ACTIVATED
========================================

BLOCKED commands (use CI instead):
  cargo build, test, check, clippy, run, bench

ALLOWED commands:
  cargo fmt         Format code
  ci fmt            Format + show changes
  ci submit         Push + create PR + wait
  ci status         Check CI status
  ci wait           Wait for CI completion
  ci logs           View failure logs
  ci merge          Merge when green

EOF

echo "Project: $_AGENT_ENV_PROJECT_ROOT"
echo ""
echo "See AGENTS.md for the complete workflow."
echo "========================================"

# Cleanup temporary variables
unset _AGENT_ENV_SCRIPT_DIR
unset _AGENT_ENV_PROJECT_ROOT
