#!/usr/bin/env bash
# TASK 6594 — the enclave self-moderation check, as one exit code.
#
# `cargo test` reports a failed test as exit 101. This wrapper narrows that to
# the two answers the finish line asks for: 0 when every required role,
# permission class, allowed/denied direction, honest client, signature field,
# re-key, progress sample, isolation subject and central-absence sweep was
# verified, and 1 when any one of them was starved.
#
#   scripts/check-enclave-self-moderation.sh          # the whole check
#   TASK6594_STARVE=role scripts/check-enclave-self-moderation.sh
#                                                     # one dimension removed: 1
set -uo pipefail

cd "$(dirname "$0")/.."

cargo test -p ipc --test task_6594_enclave_self_moderation -- --test-threads=1 --nocapture
status=$?

if [ "$status" -eq 0 ]; then
  exit 0
fi
exit 1
