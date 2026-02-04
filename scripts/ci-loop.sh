#!/usr/bin/env bash
#
# ci-loop.sh - CI feedback loop automation for Micround
#
# Provides immediate CI feedback loop utility with polling, log fetching,
# and automated push-watch-logs workflow.
#
# Usage:
#   ./scripts/ci-loop.sh watch         # Poll CI until completion
#   ./scripts/ci-loop.sh logs [RUN_ID] # Fetch failed job logs
#   ./scripts/ci-loop.sh full          # Push + watch + logs on failure
#   ./scripts/ci-loop.sh status        # Show latest run info
#   ./scripts/ci-loop.sh help          # Show usage

set -euo pipefail

# ============================================================================
# Configuration (via environment variables)
# ============================================================================

POLL_INTERVAL="${POLL_INTERVAL:-30}"    # Seconds between CI status checks
MAX_WAIT="${MAX_WAIT:-600}"             # Max seconds before timeout

# ============================================================================
# Helper Functions
# ============================================================================

log() {
    echo "[ci-loop] $*"
}

err() {
    echo "[ci-loop] ERROR: $*" >&2
}

get_current_branch() {
    git rev-parse --abbrev-ref HEAD
}

get_latest_run_id() {
    local branch
    branch=$(get_current_branch)
    gh run list --branch "$branch" --limit 1 --json databaseId --jq '.[0].databaseId // empty'
}

# ============================================================================
# Commands
# ============================================================================

cmd_watch() {
    local run_id="${1:-}"

    if [[ -z "$run_id" ]]; then
        run_id=$(get_latest_run_id)
        if [[ -z "$run_id" ]]; then
            err "No CI runs found for branch '$(get_current_branch)'"
            return 1
        fi
        log "Watching latest run: $run_id"
    else
        log "Watching run: $run_id"
    fi

    local elapsed=0
    local status conclusion

    while [[ $elapsed -lt $MAX_WAIT ]]; do
        # Query run status
        local result
        result=$(gh run view "$run_id" --json status,conclusion 2>/dev/null) || {
            err "Failed to query run $run_id"
            return 1
        }

        status=$(echo "$result" | jq -r '.status // "unknown"')
        conclusion=$(echo "$result" | jq -r '.conclusion // "pending"')

        log "Status: $status | Conclusion: $conclusion | Elapsed: ${elapsed}s"

        if [[ "$status" == "completed" ]]; then
            if [[ "$conclusion" == "success" ]]; then
                log "✅ CI passed!"
                return 0
            else
                err "❌ CI failed with conclusion: $conclusion"
                return 1
            fi
        fi

        sleep "$POLL_INTERVAL"
        elapsed=$((elapsed + POLL_INTERVAL))
    done

    err "⏱️ Timeout after ${MAX_WAIT}s waiting for CI"
    return 2
}

cmd_logs() {
    local run_id="${1:-}"

    if [[ -z "$run_id" ]]; then
        run_id=$(get_latest_run_id)
        if [[ -z "$run_id" ]]; then
            err "No CI runs found for branch '$(get_current_branch)'"
            return 1
        fi
    fi

    log "Fetching logs for run: $run_id"

    # Try to get failed job logs first
    local failed_logs
    failed_logs=$(gh run view "$run_id" --log-failed 2>/dev/null) || true

    if [[ -n "$failed_logs" ]]; then
        echo "$failed_logs"
    else
        log "No failed logs found, fetching last 100 lines of full log..."
        gh run view "$run_id" --log 2>/dev/null | tail -100
    fi

    # Show job summary
    echo ""
    log "=== Job Summary ==="
    gh run view "$run_id" --json jobs --jq '.jobs[] | "\(.name): \(.conclusion // .status)"' 2>/dev/null || true
}

cmd_full() {
    # Pre-check: uncommitted changes
    if ! git diff --quiet || ! git diff --cached --quiet; then
        err "Uncommitted changes detected. Commit or stash first."
        return 1
    fi

    # Check if we need to push
    local branch
    branch=$(get_current_branch)

    local local_sha remote_sha
    local_sha=$(git rev-parse HEAD)
    remote_sha=$(git rev-parse "origin/$branch" 2>/dev/null || echo "")

    if [[ "$local_sha" != "$remote_sha" ]]; then
        log "Pushing to origin/$branch..."
        git push origin "$branch" || {
            err "Push failed"
            return 1
        }
        log "Waiting 5s for CI to start..."
        sleep 5
    else
        log "Already up to date with origin/$branch"
    fi

    # Watch for completion
    if ! cmd_watch; then
        local exit_code=$?
        log "CI failed or timed out, fetching logs..."
        cmd_logs || true
        return $exit_code
    fi

    return 0
}

cmd_status() {
    local run_id
    run_id=$(get_latest_run_id)

    if [[ -z "$run_id" ]]; then
        log "No CI runs found for branch '$(get_current_branch)'"
        return 0
    fi

    log "Latest run for branch '$(get_current_branch)':"
    gh run view "$run_id"
}

cmd_help() {
    cat <<'EOF'
ci-loop.sh - CI feedback loop automation

COMMANDS:
  watch [RUN_ID]     Poll CI until completion (default: latest run)
  logs [RUN_ID]      Fetch failed job logs (default: latest run)
  full               Push + watch + logs on failure (automated workflow)
  status             Show latest run info
  help               Show this help message

ENVIRONMENT VARIABLES:
  POLL_INTERVAL      Seconds between CI status checks (default: 30)
  MAX_WAIT           Max seconds before timeout (default: 600)

EXIT CODES:
  0                  CI passed / success
  1                  CI failed / error
  2                  Timeout waiting for CI

EXAMPLES:
  # Watch the latest CI run until it completes
  ./scripts/ci-loop.sh watch

  # Full workflow: push, wait for CI, show logs on failure
  ./scripts/ci-loop.sh full

  # Fetch logs from a specific run
  ./scripts/ci-loop.sh logs 12345678

  # Quick status check
  ./scripts/ci-loop.sh status

  # Watch with custom timeout (5 minutes)
  MAX_WAIT=300 ./scripts/ci-loop.sh watch

REQUIREMENTS:
  - gh (GitHub CLI) installed and authenticated
  - jq (JSON processor)
  - git

EOF
}

# ============================================================================
# Main Entry Point
# ============================================================================

main() {
    # Verify dependencies
    command -v gh >/dev/null 2>&1 || { err "gh (GitHub CLI) is required"; exit 1; }
    command -v jq >/dev/null 2>&1 || { err "jq is required"; exit 1; }
    command -v git >/dev/null 2>&1 || { err "git is required"; exit 1; }

    local cmd="${1:-help}"
    shift || true

    case "$cmd" in
        watch)
            cmd_watch "$@"
            ;;
        logs)
            cmd_logs "$@"
            ;;
        full)
            cmd_full "$@"
            ;;
        status)
            cmd_status "$@"
            ;;
        help|--help|-h)
            cmd_help
            ;;
        *)
            err "Unknown command: $cmd"
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
