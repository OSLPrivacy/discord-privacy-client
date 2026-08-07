#!/usr/bin/env bash
set -euo pipefail

repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo/scripts/qa/linux-fixed-screen-launcher.sh"
capture="$repo/scripts/qa/capture-linux-osl-window.sh"
display="${OSL_FIXED_SCREEN_CAPTURE_TEST_DISPLAY:-:92}"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

image="$tmp_dir/osl-window.png"
green_out="$tmp_dir/green.out"
red_out="$tmp_dir/red.out"
red_err="$tmp_dir/red.err"
wrong_size_out="$tmp_dir/wrong-size.out"
wrong_size_err="$tmp_dir/wrong-size.err"

"$launcher" --display "$display" -- bash -c '
  set -euo pipefail
  capture="$1"
  image="$2"
  green_out="$3"
  red_out="$4"
  red_err="$5"
  wrong_size_out="$6"
  wrong_size_err="$7"

  xmessage -name "OSL Privacy" -title "OSL Privacy" -geometry 1280x800+0+0 "TASK0024 OSL window" >/dev/null 2>&1 &
  window_pid="$!"
  cleanup_window() {
    kill "$window_pid" 2>/dev/null || true
    wait "$window_pid" 2>/dev/null || true
  }
  trap cleanup_window EXIT

  for _ in $(seq 1 50); do
    if xwininfo -name "OSL Privacy" >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done

  "$capture" --output "$image" --wait-ms 5000 >"$green_out"

  set +e
  "$capture" --output "$image.wrong.png" --expected-size 1279x800 --wait-ms 500 \
    >"$wrong_size_out" 2>"$wrong_size_err"
  wrong_size_rc="$?"
  set -e
  if [ "$wrong_size_rc" -ne 1 ]; then
    printf "TASK0024_WRONG_SIZE_DID_NOT_EXIT_1 rc=%s\n" "$wrong_size_rc" >&2
    exit 1
  fi

  cleanup_window
  trap - EXIT

  set +e
  "$capture" --output "$image.missing.png" --wait-ms 250 >"$red_out" 2>"$red_err"
  missing_rc="$?"
  set -e
  if [ "$missing_rc" -ne 1 ]; then
    printf "TASK0024_MISSING_WINDOW_DID_NOT_EXIT_1 rc=%s\n" "$missing_rc" >&2
    exit 1
  fi

  printf "TASK0024_WRONG_SIZE_EXIT rc=%s\n" "$wrong_size_rc"
  printf "TASK0024_MISSING_OSL_WINDOW_EXIT rc=%s\n" "$missing_rc"
' bash "$capture" "$image" "$green_out" "$red_out" "$red_err" "$wrong_size_out" "$wrong_size_err"

cat "$green_out"
cat "$wrong_size_out"
cat "$wrong_size_err"
cat "$red_out"
cat "$red_err"

require_line() {
  local pattern="$1"
  local file="$2"
  if ! grep -Fq -- "$pattern" "$file"; then
    printf 'missing required line: %s\n' "$pattern" >&2
    exit 1
  fi
}

require_line "OSL_LINUX_WINDOW_CAPTURE" "$green_out"
require_line "output=$image" "$green_out"
require_line "size=1280x800" "$green_out"
require_line "linux-osl-window-capture: size check failed: expected 1279x800 got 1280x800" "$wrong_size_err"
require_line "linux-osl-window-capture: missing OSL window titled OSL Privacy" "$red_err"

if [ ! -s "$image" ]; then
  printf 'TASK0024_IMAGE_EMPTY path=%s\n' "$image" >&2
  exit 1
fi

read -r image_width image_height < <(identify -format '%w %h\n' "$image")
bytes="$(stat -c '%s' "$image")"

if [ "${image_width}x${image_height}" != "1280x800" ]; then
  printf 'TASK0024_IMAGE_SIZE_MISMATCH size=%sx%s\n' "$image_width" "$image_height" >&2
  exit 1
fi

printf 'TASK0024_IMAGE_NON_EMPTY bytes=%s\n' "$bytes"
printf 'TASK0024_IMAGE_FIXED_SIZE width=%s height=%s size=%sx%s\n' \
  "$image_width" \
  "$image_height" \
  "$image_width" \
  "$image_height"
