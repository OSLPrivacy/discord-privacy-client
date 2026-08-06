#!/usr/bin/env bash
set -euo pipefail

image=""
switches=""
expected_size="${OSL_TASK0027_EXPECTED_SIZE:-1280x800}"
min_distinct_colors="${OSL_TASK0027_MIN_DISTINCT_COLORS:-64}"

usage() {
  printf 'usage: %s --image PATH --switches PATH [--expected-size WIDTHxHEIGHT]\n' "$0" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --image)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      image="$2"
      shift 2
      ;;
    --switches)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      switches="$2"
      shift 2
      ;;
    --expected-size)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      expected_size="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      exit 64
      ;;
  esac
done

if [ -z "$image" ] || [ -z "$switches" ]; then
  usage
  exit 64
fi

for tool in identify stat sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'TASK0027_CAPTURE_CHECK_MISSING_TOOL tool=%s\n' "$tool" >&2
    exit 69
  fi
done

if [ ! -s "$image" ]; then
  printf 'TASK0027_CAPTURE_CHECK_FAIL reason=image-missing-or-empty path=%s\n' "$image" >&2
  exit 1
fi

if [ ! -s "$switches" ]; then
  printf 'TASK0027_CAPTURE_CHECK_FAIL reason=switch-list-missing-or-empty path=%s\n' "$switches" >&2
  exit 1
fi

expected_switches="$(mktemp)"
cleanup() {
  rm -f "$expected_switches"
}
trap cleanup EXIT

printf 'password_screen_access=require-password-screen\nsafe_sending=live-send-requires-authority\n' >"$expected_switches"
if ! cmp -s "$expected_switches" "$switches"; then
  printf 'TASK0027_CAPTURE_CHECK_FAIL reason=switch-list-mismatch path=%s\n' "$switches" >&2
  exit 1
fi

read -r width height distinct_colors < <(identify -format '%w %h %k\n' "$image")
actual_size="${width}x${height}"
if [ "$actual_size" != "$expected_size" ]; then
  printf 'TASK0027_CAPTURE_CHECK_FAIL reason=image-size expected=%s actual=%s path=%s\n' \
    "$expected_size" "$actual_size" "$image" >&2
  exit 1
fi

if [ "$distinct_colors" -lt "$min_distinct_colors" ]; then
  printf 'TASK0027_CAPTURE_CHECK_FAIL reason=blank-or-low-detail distinct_colors=%s minimum=%s path=%s\n' \
    "$distinct_colors" "$min_distinct_colors" "$image" >&2
  exit 1
fi

bytes="$(stat -c '%s' "$image")"
sha256="$(sha256sum "$image" | awk '{print $1}')"
switch_lines="$(wc -l <"$switches")"

printf 'TASK0027_CAPTURE_CHECK_PASS image=%s switches=%s bytes=%s sha256=%s size=%s distinct_colors=%s switch_lines=%s\n' \
  "$image" \
  "$switches" \
  "$bytes" \
  "$sha256" \
  "$actual_size" \
  "$distinct_colors" \
  "$switch_lines"
