#!/usr/bin/env bash
set -euo pipefail

title="OSL Privacy"
output=""
expected_size="${OSL_LINUX_CAPTURE_EXPECTED_SIZE:-${OSL_FIXED_SCREEN_SIZE:-1280x800}}"
wait_ms="5000"

usage() {
  printf 'usage: %s --output PATH [--title TITLE] [--expected-size WIDTHxHEIGHT] [--wait-ms N]\n' "$0" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      output="$2"
      shift 2
      ;;
    --title)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      title="$2"
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
    --wait-ms)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      wait_ms="$2"
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

if [ -z "$output" ]; then
  usage
  exit 64
fi

if [ -z "${DISPLAY:-}" ]; then
  printf 'linux-osl-window-capture: missing DISPLAY\n' >&2
  exit 1
fi

if ! printf '%s\n' "$expected_size" | grep -Eq '^[1-9][0-9]*x[1-9][0-9]*$'; then
  printf 'linux-osl-window-capture: invalid expected size: %s\n' "$expected_size" >&2
  exit 64
fi

if ! printf '%s\n' "$wait_ms" | grep -Eq '^[0-9]+$'; then
  printf 'linux-osl-window-capture: invalid wait-ms: %s\n' "$wait_ms" >&2
  exit 64
fi

for tool in xdotool xwininfo xwd convert identify stat; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'linux-osl-window-capture: missing required tool: %s\n' "$tool" >&2
    exit 69
  fi
done

deadline=$((SECONDS + (wait_ms + 999) / 1000))
window_id=""
while :; do
  mapfile -t candidates < <(xdotool search --onlyvisible --name "$title" 2>/dev/null || true)
  matches=()
  for candidate in "${candidates[@]}"; do
    candidate_title="$(xdotool getwindowname "$candidate" 2>/dev/null || true)"
    if [ "$candidate_title" = "$title" ]; then
      matches+=("$candidate")
    fi
  done

  if [ "${#matches[@]}" -eq 1 ]; then
    window_id="${matches[0]}"
    break
  fi

  if [ "${#matches[@]}" -gt 1 ]; then
    printf 'linux-osl-window-capture: multiple visible OSL windows titled %s\n' "$title" >&2
    exit 1
  fi

  if [ "$SECONDS" -ge "$deadline" ]; then
    printf 'linux-osl-window-capture: missing OSL window titled %s\n' "$title" >&2
    exit 1
  fi
  sleep 0.1
done

window_info="$(xwininfo -id "$window_id" 2>/dev/null)" || {
  printf 'linux-osl-window-capture: OSL window disappeared before geometry read\n' >&2
  exit 1
}

width="$(printf '%s\n' "$window_info" | awk '/Width:/{print $2; exit}')"
height="$(printf '%s\n' "$window_info" | awk '/Height:/{print $2; exit}')"
actual_size="${width}x${height}"
if [ "$actual_size" != "$expected_size" ]; then
  printf 'linux-osl-window-capture: size check failed: expected %s got %s\n' \
    "$expected_size" "${actual_size:-missing}" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

raw_capture="$tmp_dir/osl-window.xwd"
png_capture="$tmp_dir/osl-window.png"

xwd -silent -id "$window_id" -out "$raw_capture"
convert "$raw_capture" "$png_capture"

if [ ! -s "$png_capture" ]; then
  printf 'linux-osl-window-capture: capture image is empty\n' >&2
  exit 1
fi

read -r image_width image_height < <(identify -format '%w %h\n' "$png_capture")
image_size="${image_width}x${image_height}"
if [ "$image_size" != "$expected_size" ]; then
  printf 'linux-osl-window-capture: image size check failed: expected %s got %s\n' \
    "$expected_size" "$image_size" >&2
  exit 1
fi

output_dir="$(dirname -- "$output")"
mkdir -p "$output_dir"
install -m 0644 "$png_capture" "$output"
bytes="$(stat -c '%s' "$output")"
if [ "$bytes" -le 0 ]; then
  printf 'linux-osl-window-capture: named image is empty: %s\n' "$output" >&2
  exit 1
fi

printf 'OSL_LINUX_WINDOW_CAPTURE window_id=%s output=%s bytes=%s size=%s\n' \
  "$window_id" \
  "$output" \
  "$bytes" \
  "$image_size"
