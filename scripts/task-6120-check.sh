#!/usr/bin/env bash
# TASK 6120 — run the deployed OSL Chats authorization check.
#
# Builds and deploys the shipping `osl-chats-service` / `osl-chats-client`
# binaries for whatever source tree is on disk right now, drives the five
# installed clients against the deployed service and judges the bytes they got
# back together with the service's own journal.
#
# Exits 0 when every authorized control succeeded and every forbidden path
# returned nothing, and exits 1 — never any other code — otherwise, so a
# throwaway mutated build is distinguishable from a broken harness.
#
#   scripts/task-6120-check.sh [build-tag]
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_TAG="${1:-production}"
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the target directory for this lane}"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$REPO_ROOT" || exit 1

log="$(mktemp -t task6120-check-XXXXXX.log)"
TASK6120_BUILD_TAG="$BUILD_TAG" cargo test --locked -j"${TASK6120_JOBS:-4}" \
  --manifest-path crates/ipc/Cargo.toml \
  -p ipc --test task_6120_deployed_chats_authorization \
  -- --nocapture --test-threads=1 >"$log" 2>&1
status=$?

grep -E '^TASK6120 (INSTALL|FROZEN|DEPLOYED|SUMMARY|FAIL|RESULT)' "$log" || true

if [ "$status" -ne 0 ]; then
  if ! grep -q '^TASK6120 RESULT' "$log"; then
    echo "TASK6120 FAIL the check did not run to a verdict; full log at $log"
    sed -n '1,80p' "$log"
  fi
  echo "TASK6120 CHECK exit=1 tag=$BUILD_TAG log=$log"
  exit 1
fi

echo "TASK6120 CHECK exit=0 tag=$BUILD_TAG log=$log"
exit 0
