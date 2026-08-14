#!/usr/bin/env bash
# TASK 6206 — run the deployed OSL Chats content-write authorization check.
#
# Builds and deploys the shipping `osl-chats-service` / `osl-chats-client`
# binaries for whatever source tree is on disk right now, derives the inventory
# from the installed client's route manifest and the deployed service's router,
# drives an authorized control and a hostile signed request against every row
# from five independently keyed installs, and judges the bytes each client got
# back together with the deployed service's own durable records, delivery queue,
# broadcast log and decision journal.
#
# Exits 0 when every authorized control changed exactly its frozen target and
# every hostile write was refused before a durable, queued or broadcast byte,
# and exits 1 — never any other code — otherwise, so a throwaway mutated build
# is distinguishable from a broken harness.
#
#   scripts/task-6206-check.sh [build-tag]
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_TAG="${1:-production}"
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the target directory for this lane}"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$REPO_ROOT" || exit 1

log="$(mktemp -t task6206-check-XXXXXX.log)"
TASK6206_BUILD_TAG="$BUILD_TAG" cargo test --locked -j"${TASK6206_JOBS:-4}" \
  --manifest-path crates/ipc/Cargo.toml \
  -p ipc --test task_6206_deployed_chats_content_writes \
  -- --nocapture --test-threads=1 >"$log" 2>&1
status=$?

grep -E '^TASK6206 (INSTALL|FROZEN|DEPLOYED|INVENTORY|SUMMARY|FAIL|RESULT)' "$log" || true

if [ "$status" -ne 0 ]; then
  if ! grep -q '^TASK6206 RESULT' "$log"; then
    echo "TASK6206 FAIL the check did not run to a verdict; full log at $log"
    sed -n '1,80p' "$log"
  fi
  echo "TASK6206 CHECK exit=1 tag=$BUILD_TAG log=$log"
  exit 1
fi

echo "TASK6206 CHECK exit=0 tag=$BUILD_TAG log=$log"
exit 0
