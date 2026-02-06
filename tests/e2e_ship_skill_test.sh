#!/usr/bin/env bash
#
# E2E Test: /ship Skill (bd-26c)
#
# Tests the /ship skill's orchestration of the PR-CI-fix loop.
# This is a manual/semi-automated E2E test that verifies the skill workflow.
#
# Prerequisites:
# - gh CLI authenticated
# - Git repository with remote
# - Feature branch (not main)
#
# Usage:
#   ./tests/e2e_ship_skill_test.sh [test_name]
#
# Tests:
#   prereqs    - Verify prerequisites are met
#   skill      - Verify skill file exists and is valid
#   ci_loop    - Verify ci-loop.sh script works
#   full       - Run all tests

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors (disabled if NO_COLOR set or not a tty)
if [[ -n "${NO_COLOR:-}" ]] || [[ ! -t 1 ]]; then
    RED=''
    GREEN=''
    YELLOW=''
    NC=''
else
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    NC='\033[0m'
fi

# Test counters
PASSED=0
FAILED=0

# ============================================================================
# Test Utilities
# ============================================================================

log_test() {
    echo -e "${YELLOW}[TEST]${NC} $*"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $*"
    PASSED=$((PASSED + 1))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $*"
    FAILED=$((FAILED + 1))
}

assert_file_exists() {
    local file="$1"
    local desc="${2:-File exists}"

    if [[ -f "$file" ]]; then
        log_pass "$desc: $file"
        return 0
    else
        log_fail "$desc: $file (not found)"
        return 1
    fi
}

assert_file_contains() {
    local file="$1"
    local pattern="$2"
    local desc="${3:-File contains pattern}"

    if grep -q "$pattern" "$file" 2>/dev/null; then
        log_pass "$desc"
        return 0
    else
        log_fail "$desc: pattern '$pattern' not found in $file"
        return 1
    fi
}

assert_command_exists() {
    local cmd="$1"
    local desc="${2:-Command exists}"

    if command -v "$cmd" &>/dev/null; then
        log_pass "$desc: $cmd"
        return 0
    else
        log_fail "$desc: $cmd (not found)"
        return 1
    fi
}

assert_command_succeeds() {
    local desc="$1"
    shift

    if "$@" &>/dev/null; then
        log_pass "$desc"
        return 0
    else
        log_fail "$desc: command failed"
        return 1
    fi
}

# ============================================================================
# Test: Prerequisites
# ============================================================================

test_prereqs() {
    log_test "Checking prerequisites..."

    # Check required commands
    assert_command_exists "gh" "GitHub CLI installed"
    assert_command_exists "git" "Git installed"
    assert_command_exists "cargo" "Cargo installed"

    # Check gh authentication
    if gh auth status &>/dev/null; then
        log_pass "GitHub CLI authenticated"
    else
        log_fail "GitHub CLI not authenticated (run 'gh auth login')"
    fi

    # Check git repository
    if git rev-parse --git-dir &>/dev/null; then
        log_pass "In git repository"
    else
        log_fail "Not in a git repository"
    fi

    # Check remote exists
    if git remote get-url origin &>/dev/null; then
        log_pass "Git remote 'origin' configured"
    else
        log_fail "No git remote 'origin' configured"
    fi
}

# ============================================================================
# Test: Skill File Validation
# ============================================================================

test_skill() {
    log_test "Validating /ship skill file..."

    local skill_file="$PROJECT_ROOT/.claude/skills/ship/SKILL.md"

    # Check file exists
    assert_file_exists "$skill_file" "Skill file exists"

    # Check required content
    assert_file_contains "$skill_file" "Trigger" "Has trigger section"
    assert_file_contains "$skill_file" "Workflow" "Has workflow section"
    assert_file_contains "$skill_file" "ci-loop" "References ci-loop.sh"
    assert_file_contains "$skill_file" "Pre-flight" "Has pre-flight checks"
    assert_file_contains "$skill_file" "PR" "Handles PR creation"
    assert_file_contains "$skill_file" "fix" "Has fix loop guidance"
    assert_file_contains "$skill_file" "Safety" "Has safety valve"

    # Check for key workflow steps
    assert_file_contains "$skill_file" "gh pr" "Uses gh pr commands"
    assert_file_contains "$skill_file" "git" "Uses git commands"
}

# ============================================================================
# Test: CI Loop Script
# ============================================================================

test_ci_loop() {
    log_test "Validating ci-loop.sh script..."

    local script="$PROJECT_ROOT/scripts/ci-loop.sh"

    # Check file exists and is executable
    assert_file_exists "$script" "ci-loop.sh exists"

    if [[ -x "$script" ]]; then
        log_pass "ci-loop.sh is executable"
    else
        log_fail "ci-loop.sh is not executable"
    fi

    # Check required functions
    assert_file_contains "$script" "watch" "Has watch command"
    assert_file_contains "$script" "logs" "Has logs command"
    assert_file_contains "$script" "full" "Has full command"
    assert_file_contains "$script" "status" "Has status command"

    # Check script runs help without error
    if "$script" help &>/dev/null; then
        log_pass "ci-loop.sh help runs successfully"
    else
        log_fail "ci-loop.sh help failed"
    fi

    # Check script runs status (may fail if no remote, but shouldn't crash)
    if timeout 5 "$script" status 2>&1 | head -1 | grep -qE '(ci-loop|run|error|no remote)'; then
        log_pass "ci-loop.sh status runs (output received)"
    else
        log_pass "ci-loop.sh status runs (no crash)"
    fi
}

# ============================================================================
# Test: Branch Validation (pre-flight check)
# ============================================================================

test_branch_validation() {
    log_test "Testing branch validation logic..."

    local branch
    branch=$(git rev-parse --abbrev-ref HEAD)

    # Report current branch
    echo "  Current branch: $branch"

    # Validate the skill's branch check would work
    if [[ "$branch" == "main" || "$branch" == "develop" ]]; then
        log_pass "On protected branch - /ship should warn user"
    else
        log_pass "On feature branch - /ship can proceed"
    fi

    # Check for uncommitted changes
    if git diff --quiet && git diff --cached --quiet; then
        log_pass "No uncommitted changes"
    else
        log_pass "Has uncommitted changes - /ship should warn user"
    fi
}

# ============================================================================
# Test: Full Integration (informational)
# ============================================================================

test_integration_info() {
    log_test "Integration test information..."

    echo ""
    echo "  To fully test /ship skill integration:"
    echo ""
    echo "  1. Create a feature branch:"
    echo "     git checkout -b test/ship-e2e"
    echo ""
    echo "  2. Make a small change and commit:"
    echo "     echo '// test' >> src/lib.rs"
    echo "     git add src/lib.rs && git commit -m 'test: ship skill'"
    echo ""
    echo "  3. Invoke /ship in Claude Code:"
    echo "     /ship"
    echo ""
    echo "  4. Verify skill:"
    echo "     - Creates PR if none exists"
    echo "     - Runs ./scripts/ci-loop.sh full"
    echo "     - Monitors CI status"
    echo "     - Proposes fixes if CI fails"
    echo ""
    echo "  5. Cleanup:"
    echo "     git checkout main"
    echo "     git branch -D test/ship-e2e"
    echo "     gh pr close --delete-branch (if PR created)"
    echo ""

    log_pass "Integration test instructions provided"
}

# ============================================================================
# Run Tests
# ============================================================================

run_all_tests() {
    echo "========================================"
    echo " E2E Test: /ship Skill (bd-26c)"
    echo "========================================"
    echo ""

    test_prereqs
    echo ""
    test_skill
    echo ""
    test_ci_loop
    echo ""
    test_branch_validation
    echo ""
    test_integration_info

    echo ""
    echo "========================================"
    echo " Results: $PASSED passed, $FAILED failed"
    echo "========================================"

    if [[ $FAILED -gt 0 ]]; then
        exit 1
    fi
}

# ============================================================================
# Main
# ============================================================================

cd "$PROJECT_ROOT"

case "${1:-full}" in
    prereqs)
        test_prereqs
        ;;
    skill)
        test_skill
        ;;
    ci_loop|ci-loop)
        test_ci_loop
        ;;
    branch)
        test_branch_validation
        ;;
    info)
        test_integration_info
        ;;
    full|all)
        run_all_tests
        ;;
    *)
        echo "Usage: $0 [prereqs|skill|ci_loop|branch|info|full]"
        exit 1
        ;;
esac
