#!/usr/bin/env bash
set -euo pipefail

repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
runner="$repo/scripts/qa/run-linux-osl-on-fixed-screen.sh"
display="${OSL_FIXED_SCREEN_TEST_DISPLAY:-:90}"
switch_one="password_screen_access=skip-password-screen-for-test"
switch_two="safe_sending=dry-run-send-for-test"
target_dir="${CARGO_TARGET_DIR:-$repo/.cargo-target}"
example="$target_dir/debug/examples/runtime_switch_reader"

CARGO_TARGET_DIR="$target_dir" osl-cargo build --locked \
  --manifest-path "$repo/apps/osl-hub/Cargo.toml" \
  --no-default-features \
  --features core \
  --example runtime_switch_reader

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

record="$tmp_dir/osl-run.record"
green_out="$tmp_dir/green.out"
"$runner" \
  --display "$display" \
  --record "$record" \
  --switch "$switch_one" \
  --switch "$switch_two" \
  -- "$example" >"$green_out"

cat "$green_out"
cat "$record"

require_line() {
  local pattern="$1"
  local file="$2"
  if ! grep -Fq -- "$pattern" "$file"; then
    printf 'missing required line: %s\n' "$pattern" >&2
    exit 1
  fi
}

require_line "OSL_FIXED_SCREEN display=$display size=1280x800 color_depth=24 scale=1 theme=dark dpi=96x96" "$green_out"
require_line "RUN-TIME SWITCH LIST: osl-test-only-runtime-switches" "$green_out"
require_line "OLD CHOICE password_screen_access=skip-password-screen-for-test source=run-time switches" "$green_out"
require_line "OLD CHOICE safe_sending=dry-run-send-for-test source=run-time switches" "$green_out"
require_line "switches=$switch_one $switch_two" "$record"
require_line "display=$display" "$record"
require_line "size=1280x800" "$record"
require_line "scale=1" "$record"
require_line "theme=dark" "$record"

process_number="$(sed -n 's/^process_number=//p' "$record" | head -n 1)"
if ! printf '%s\n' "$process_number" | grep -Eq '^[0-9]+$'; then
  printf 'process number was not numeric: %s\n' "$process_number" >&2
  exit 1
fi

printf 'TASK0023_OPENED_ON_FAKE_SCREEN process_number=%s switches="%s %s"\n' \
  "$process_number" \
  "$switch_one" \
  "$switch_two"

missing_launcher="$tmp_dir/missing-display-launcher.sh"
cat >"$missing_launcher" <<'SHIM'
#!/usr/bin/env bash
set -euo pipefail
while [ "$#" -gt 0 ]; do
  case "$1" in
    --display)
      shift 2
      ;;
    --)
      shift
      break
      ;;
    *)
      break
      ;;
  esac
done
unset DISPLAY
"$@"
SHIM
chmod +x "$missing_launcher"

missing_out="$tmp_dir/missing.out"
missing_err="$tmp_dir/missing.err"
set +e
OSL_LINUX_FIXED_SCREEN_LAUNCHER="$missing_launcher" "$runner" \
  --record "$tmp_dir/missing.record" \
  --switch "$switch_one" \
  --switch "$switch_two" \
  -- "$example" >"$missing_out" 2>"$missing_err"
missing_rc="$?"
set -e

cat "$missing_out"
cat "$missing_err"

if [ "$missing_rc" -ne 1 ]; then
  printf 'MISSING_DISPLAY_DID_NOT_EXIT_1 rc=%s\n' "$missing_rc" >&2
  exit 1
fi

require_line "linux-osl-fixed-screen-run: missing DISPLAY" "$missing_err"
printf 'TASK0023_MISSING_DISPLAY_EXIT rc=%s\n' "$missing_rc"
