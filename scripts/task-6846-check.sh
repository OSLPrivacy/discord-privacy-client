#!/usr/bin/env bash
set -u

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/i}"
export RUSTC_WRAPPER=
if cargo test -j 1 -p crypto --test task_6846_voice_panel -- --test-threads=1 --nocapture; then
  exit 0
fi
exit 1
