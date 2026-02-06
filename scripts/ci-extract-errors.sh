#!/usr/bin/env bash
#
# ci-extract-errors.sh - Extract actionable errors from Rust CI logs
#
# Filters verbose CI output into focused, actionable error messages.
# Designed to keep output under 50 lines total.
#
# Usage:
#   gh run view $RUN --log-failed | ./scripts/ci-extract-errors.sh
#   cat ci-log.txt | ./scripts/ci-extract-errors.sh

set -euo pipefail

# Limits per category to keep total output <50 lines
COMPILER_ERROR_LIMIT=15
TEST_FAILURE_LIMIT=15
CLIPPY_ERROR_LIMIT=10
LINKER_ERROR_LIMIT=5
BUILD_FAILURE_LIMIT=5

log() {
    echo "[ci-extract] $*" >&2
}

print_section() {
    local title="$1"
    local count="$2"
    if [[ $count -gt 0 ]]; then
        echo ""
        echo "=== $title ($count found) ==="
    fi
}

count_lines() {
    local text="$1"
    if [[ -z "$text" ]]; then
        echo 0
    else
        echo "$text" | wc -l | tr -d ' '
    fi
}

main() {
    local input_file
    input_file=$(mktemp)
    trap "rm -f '$input_file'" EXIT

    cat > "$input_file"

    local total_lines
    total_lines=$(wc -l < "$input_file" | tr -d ' ')

    if [[ $total_lines -eq 0 ]]; then
        echo "No input received."
        exit 0
    fi

    log "Processing $total_lines lines of CI output..."

    # 1. Compiler errors
    local compiler_errors
    compiler_errors=$(grep -E 'error\[E[0-9]{4}\]:' "$input_file" 2>/dev/null | head -"$COMPILER_ERROR_LIMIT" || true)
    local compiler_count
    compiler_count=$(count_lines "$compiler_errors")
    if [[ -n "$compiler_errors" ]]; then
        print_section "COMPILER ERRORS" "$compiler_count"
        echo "$compiler_errors"
    fi

    # 2. Test failures
    local test_failures
    test_failures=$(grep -E '(---- .+ stdout ----)|panicked at|test .+ \.\.\. FAILED' "$input_file" 2>/dev/null | head -"$TEST_FAILURE_LIMIT" || true)
    local test_count
    test_count=$(count_lines "$test_failures")
    if [[ -n "$test_failures" ]]; then
        print_section "TEST FAILURES" "$test_count"
        echo "$test_failures"
    fi

    # 3. Clippy/lint errors
    local clippy_errors
    clippy_errors=$(grep -E '^error: ' "$input_file" 2>/dev/null | grep -v 'error\[E' | grep -v 'error: could not compile' | grep -v 'error: test failed' | head -"$CLIPPY_ERROR_LIMIT" || true)
    local clippy_count
    clippy_count=$(count_lines "$clippy_errors")
    if [[ -n "$clippy_errors" ]]; then
        print_section "CLIPPY/LINT ERRORS" "$clippy_count"
        echo "$clippy_errors"
    fi

    # 4. Linker errors
    local linker_errors
    linker_errors=$(grep -E '(undefined reference|cannot find -l|undefined symbol)' "$input_file" 2>/dev/null | head -"$LINKER_ERROR_LIMIT" || true)
    local linker_count
    linker_count=$(count_lines "$linker_errors")
    if [[ -n "$linker_errors" ]]; then
        print_section "LINKER ERRORS" "$linker_count"
        echo "$linker_errors"
    fi

    # 5. Build failures
    local build_failures
    build_failures=$(grep -E 'error: could not compile|error: aborting due to|Build failed' "$input_file" 2>/dev/null | head -"$BUILD_FAILURE_LIMIT" || true)
    local build_count
    build_count=$(count_lines "$build_failures")
    if [[ -n "$build_failures" ]]; then
        print_section "BUILD FAILURES" "$build_count"
        echo "$build_failures"
    fi

    # Summary
    local total_errors=$((compiler_count + test_count + clippy_count + linker_count + build_count))

    if [[ $total_errors -eq 0 ]]; then
        echo ""
        echo "No actionable errors found in $total_lines lines of output."
        echo "Run 'gh run view \$RUN --log' for full logs."
    else
        echo ""
        echo "--- Summary: $total_errors errors extracted from $total_lines lines ---"
    fi
}

main "$@"
