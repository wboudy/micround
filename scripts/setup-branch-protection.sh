#!/bin/bash
# setup-branch-protection.sh: Configure GitHub branch protection
#
# This script automates the configuration of branch protection rules
# for the main branch to enforce CI-first workflow.
#
# Prerequisites:
#   - Admin access to the repository
#   - gh CLI installed and authenticated
#
# Usage: ./scripts/setup-branch-protection.sh

set -euo pipefail

# Colors for output
if [ -t 1 ]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  BLUE='\033[0;34m'
  YELLOW='\033[0;33m'
  NC='\033[0m'
else
  RED=''
  GREEN=''
  BLUE=''
  YELLOW=''
  NC=''
fi

# Check gh CLI availability
if ! command -v gh &>/dev/null; then
  echo -e "${RED}ERROR: gh CLI required${NC}"
  echo "Install from: https://cli.github.com/"
  exit 1
fi

# Check authentication
if ! gh auth status &>/dev/null; then
  echo -e "${RED}ERROR: gh CLI not authenticated${NC}"
  echo "Run: gh auth login"
  exit 1
fi

# Get repo info
REPO=$(gh repo view --json nameWithOwner --jq ".nameWithOwner" 2>/dev/null || echo "")
if [ -z "$REPO" ]; then
  echo -e "${RED}ERROR: Not in a GitHub repository${NC}"
  echo "Run this from the repository root directory."
  exit 1
fi

BRANCH="main"

echo -e "${BLUE}========================================"
echo "Configuring Branch Protection"
echo "========================================${NC}"
echo ""
echo "Repository: $REPO"
echo "Branch:     $BRANCH"
echo ""

# Check if user has admin access
echo -e "${BLUE}Checking permissions...${NC}"
PERMISSION=$(gh api "/repos/$REPO" --jq ".permissions.admin" 2>/dev/null || echo "false")
if [ "$PERMISSION" != "true" ]; then
  echo -e "${YELLOW}WARNING: You may not have admin access to this repository${NC}"
  echo "Branch protection configuration requires admin permissions."
  echo ""
  read -p "Continue anyway? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
  fi
fi

echo ""
echo -e "${BLUE}Applying branch protection rules...${NC}"

# Configure branch protection using the GitHub API
# Note: Using heredoc with JSON for proper formatting
gh api --method PUT "/repos/$REPO/branches/$BRANCH/protection" \
  --input - << 'EOF'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["CI Success"]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 0
  },
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": true
}
EOF

echo ""
echo -e "${GREEN}========================================"
echo "Branch Protection Configured!"
echo "========================================${NC}"
echo ""
echo "Settings applied to '$BRANCH' branch:"
echo ""
echo "  Status Checks:"
echo "    - Require 'CI Success' to pass"
echo "    - Require branch to be up to date"
echo ""
echo "  Pull Request Rules:"
echo "    - PRs required for all changes"
echo "    - Dismiss stale approvals on new commits"
echo "    - Require conversation resolution"
echo ""
echo "  History & Pushes:"
echo "    - Require linear history (squash/rebase)"
echo "    - Block force pushes"
echo "    - Block branch deletion"
echo "    - Enforce rules for admins"
echo ""
echo "Direct pushes to $BRANCH are now BLOCKED."
echo "All changes must go through PRs with passing CI."
echo ""
echo "See docs/BRANCH_PROTECTION.md for more details."
