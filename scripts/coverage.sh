#!/usr/bin/env bash
# Run cargo-llvm-cov on the arcp crate.
#
# Why this script exists: cargo-llvm-cov needs llvm-cov / llvm-profdata,
# which are shipped by rustup's `llvm-tools-preview` component but are NOT
# included in Homebrew's rustc. When cargo is the Homebrew binary,
# llvm-cov fails to find the tools even after the rustup component is
# installed. This wrapper locates the rustup-managed binaries and exports
# them via env vars so cargo-llvm-cov uses them regardless of which cargo
# is on PATH.
#
# Usage:
#   scripts/coverage.sh                  # human-readable summary
#   scripts/coverage.sh --html           # HTML report under target/llvm-cov
#   scripts/coverage.sh --lcov --output-path coverage.lcov
#   scripts/coverage.sh --fail-under-lines 85
# Any extra args are forwarded to `cargo llvm-cov`.

set -euo pipefail

if ! command -v rustup >/dev/null 2>&1; then
    echo "error: rustup not found on PATH" >&2
    echo "install rustup from https://rustup.rs/" >&2
    exit 1
fi

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "error: cargo-llvm-cov not installed" >&2
    echo "install with: cargo install cargo-llvm-cov" >&2
    exit 1
fi

# Use rustup's sysroot, not whatever rustc is on PATH (which may be the
# Homebrew rustc that doesn't ship llvm-tools-preview).
SYSROOT=$(rustup run stable rustc --print sysroot)
HOST=$(rustup run stable rustc -vV | sed -n 's/^host: //p')
BIN_DIR="$SYSROOT/lib/rustlib/$HOST/bin"

export LLVM_COV="$BIN_DIR/llvm-cov"
export LLVM_PROFDATA="$BIN_DIR/llvm-profdata"

if [[ ! -x "$LLVM_COV" || ! -x "$LLVM_PROFDATA" ]]; then
    echo "error: $LLVM_COV or $LLVM_PROFDATA not found" >&2
    echo "install with: rustup component add llvm-tools-preview" >&2
    exit 1
fi

# The CLI binary is exercised by manual invocation, not test code; exclude
# it so it doesn't drag the floor down. The Phase 6+ transport stubs that
# ship as Phase-N-deferred surfaces are kept honest in the report.
ARGS=(
    --all-features
    --ignore-filename-regex='src/bin/'
)

# Default to a summary; the caller can pass --html / --lcov etc.
if [[ $# -eq 0 ]]; then
    ARGS+=(--summary-only)
fi

exec cargo llvm-cov "${ARGS[@]}" "$@"
