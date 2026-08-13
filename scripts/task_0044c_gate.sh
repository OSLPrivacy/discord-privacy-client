#!/usr/bin/env bash
# TASK 0044c release gate: per-attachment content-key separation and
# compromise containment.
#
# Runs the unchanged key-boundary proof and maps its result to the two exit
# codes the task's finish line is written in:
#
#   0  every attachment has its own random content key, and exposing any one
#      attachment's complete key material buys exactly that one attachment
#   1  a key-separation or compromise-containment failure (the transcript
#      names the object and the cross-object compromise)
#
# `cargo test` reports a failing Rust test as 101; this gate is what turns
# that into the required 1. Any other cargo failure (a build error, a missing
# toolchain) is reported as 2 so it can never be mistaken for a clean red.
#
# Usage: scripts/task_0044c_gate.sh [mutation-label]
set -u -o pipefail

MUTATION="${1:-none}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${REPO_ROOT}/apps/osl-hub/Cargo.toml"
TARGET="task_0044c_attachment_key_boundary"

echo "TASK0044C gate mutation=${MUTATION} manifest=apps/osl-hub/Cargo.toml target=${TARGET}"

set +e
cargo test \
  --manifest-path "${MANIFEST}" \
  --no-default-features \
  --features core \
  --test "${TARGET}" \
  -- --nocapture --test-threads=1
CARGO_STATUS=$?
set -e

echo "TASK0044C gate cargo_status=${CARGO_STATUS}"

case "${CARGO_STATUS}" in
  0)
    echo "TASK0044C gate mutation=${MUTATION} result=green exit=0"
    exit 0
    ;;
  101)
    echo "TASK0044C gate mutation=${MUTATION} result=red exit=1"
    exit 1
    ;;
  *)
    echo "TASK0044C gate mutation=${MUTATION} result=inconclusive exit=2"
    exit 2
    ;;
esac
