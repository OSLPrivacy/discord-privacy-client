#!/usr/bin/env bash
set -u -o pipefail

fault="${1:-}"
case "$fault" in
  "") export TASK4121_FAULT="" ;;
  starve-email-20|omit-shipping-pointer-reader) export TASK4121_FAULT="$fault" ;;
  *) echo "TASK4121 unknown fault: $fault" >&2; exit 2 ;;
esac

set +e
CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i cargo test --manifest-path apps/osl-hub/Cargo.toml --no-default-features --features core --test task_4121_gmail_pointer_readback -- --test-threads=1 --nocapture
status=$?
set -e
if [ -z "$fault" ]; then
  exit "$status"
fi
if [ "$status" -eq 0 ]; then
  echo "TASK4121 fault=$fault unexpectedly passed" >&2
  exit 1
fi
exit 1
