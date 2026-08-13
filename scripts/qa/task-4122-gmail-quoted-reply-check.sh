#!/usr/bin/env bash
set -u -o pipefail

fault="${1:-}"
case "$fault" in
  "") export TASK4122_OMIT_QUOTED_BLOCK_EXCLUSION="" ;;
  omit-quoted-block-exclusion) export TASK4122_OMIT_QUOTED_BLOCK_EXCLUSION="1" ;;
  *) echo "TASK4122 unknown fault: $fault" >&2; exit 2 ;;
esac

set +e
CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/c cargo test --manifest-path apps/osl-hub/Cargo.toml --no-default-features --features core --test task_4122_gmail_quoted_reply_readback -- --test-threads=1 --nocapture
status=$?
set -e
if [ -z "$fault" ]; then
  exit "$status"
fi
if [ "$status" -eq 0 ]; then
  echo "TASK4122 fault=$fault unexpectedly passed" >&2
  exit 1
fi
exit 1
