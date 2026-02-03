#!/bin/bash
# setup-hooks.sh: Install git hooks for CI-first workflow
#
# Run this once per clone to enable the pre-commit and pre-merge-commit hooks.
# These hooks help enforce formatting and CI-first development practices.
#
# Usage: ./scripts/setup-hooks.sh

set -e

# Colors for output
if [ -t 1 ]; then
  GREEN='\033[0;32m'
  BLUE='\033[0;34m'
  NC='\033[0m'
else
  GREEN=''
  BLUE=''
  NC=''
fi

# Determine script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo -e "${BLUE}Setting up git hooks...${NC}"
echo ""

# Verify we're in a git repo
if ! git rev-parse --git-dir &>/dev/null; then
  echo "ERROR: Not in a git repository"
  exit 1
fi

# Ensure .githooks directory exists
if [ ! -d "$PROJECT_ROOT/.githooks" ]; then
  echo "ERROR: .githooks directory not found"
  echo "Expected: $PROJECT_ROOT/.githooks"
  exit 1
fi

# Configure git to use our hooks directory
git config core.hooksPath .githooks

echo "Configured core.hooksPath = .githooks"

# Ensure all hooks are executable
chmod +x "$PROJECT_ROOT/.githooks/"* 2>/dev/null || true

echo ""
echo -e "${GREEN}Git hooks installed successfully!${NC}"
echo ""
echo "Active hooks in .githooks/:"
ls -la "$PROJECT_ROOT/.githooks/" 2>/dev/null | grep -v "^total" | grep -v "^\." || echo "  (none found)"
echo ""
echo "Hooks will now run on:"
echo "  - pre-commit        Format check before each commit"
echo "  - pre-merge-commit  CI check before local merges"
echo ""
echo "To verify: git config core.hooksPath"
echo "Output should be: .githooks"
