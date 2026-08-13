#!/usr/bin/env bash
# TASK 6856 — the customisable Enclave layout check, as one exit code.
#
# `cargo test` reports a failed test as exit 101. This wrapper narrows that to
# the two answers the finish line asks for: 0 when every required dimension was
# verified, 1 when anything was starved, refused or resolved wrongly.
#
#   scripts/check-enclave-layout.sh                 # the whole check
#   OSL_TASK_6856_STARVE=mode:stewards scripts/check-enclave-layout.sh
#                                                   # one dimension removed: 1
set -uo pipefail

cd "$(dirname "$0")/.."

cargo test -p ipc --lib task_6856 -- --test-threads=1 --nocapture
status=$?

if [ "$status" -eq 0 ]; then
  exit 0
fi
exit 1
