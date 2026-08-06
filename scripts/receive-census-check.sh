#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXPECTED_TARGET="/mnt/d/osl-lane-targets/f"
if [[ "${CARGO_TARGET_DIR:-}" != "$EXPECTED_TARGET" ]]; then
  echo "FAIL: CARGO_TARGET_DIR must be $EXPECTED_TARGET"
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

(
  cd "$ROOT/apps/osl-hub"
  cargo test \
    --locked \
    --no-default-features \
    --features core \
    --test native_discord_receive_e2e \
    discord_receiving_command_refuses_empty_store \
    -- --exact --test-threads=1 --nocapture
) >"$tmp" 2>&1
status=$?

cat "$tmp"
if [[ "$status" -eq 0 ]]; then
  echo "OK: Discord empty-store receive returned no private content."
  exit 0
fi

if grep -Fq "Discord answered with no content behind it" "$tmp"; then
  echo "FAIL: Discord answered with no content behind it"
else
  echo "FAIL: Discord empty-store receive gate failed without the expected diagnostic"
fi
exit 1
