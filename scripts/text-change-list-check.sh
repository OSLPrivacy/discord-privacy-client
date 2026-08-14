#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXPECTED_TARGET="/mnt/d/osl-lane-targets/m"
if [[ "${CARGO_TARGET_DIR:-}" != "$EXPECTED_TARGET" ]]; then
  echo "FAIL: CARGO_TARGET_DIR must be $EXPECTED_TARGET"
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

(
  cd "$ROOT"
  cargo test \
    --manifest-path apps/osl-hub/task_3950_text_change_list/Cargo.toml \
    --locked \
    --test text_change_list \
    task_3950_text_change_list_matches_known_app_readbacks \
    -- --exact --test-threads=1 --nocapture
) >"$tmp" 2>&1
status=$?

cat "$tmp"
if [[ "$status" -eq 0 ]]; then
  echo "OK: TASK3950 text-change list names every observed app change."
  exit 0
fi

if grep -Fq "read-back matched sent text exactly when the app is known to change it" "$tmp"; then
  app="$(sed -n 's/.*TASK3950_FAILURE app=\([^ ]*\) read-back matched sent text exactly.*/\1/p' "$tmp" | head -1)"
  echo "FAIL: TASK3950 app=${app:-unknown} read-back matched sent text exactly when the app is known to change it"
else
  echo "FAIL: TASK3950 text-change list failed without the expected diagnostic"
fi
exit 1
