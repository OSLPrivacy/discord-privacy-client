#!/usr/bin/env bash
set -u -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

SCREEN="${OSL_FIXED_SCREEN_SIZE:-1440x900x24}"
DISPLAY_ID="${OSL_FIXED_SCREEN_DISPLAY:-:$((60 + ($$ % 30)))}"
OUT_DIR="${OSL_FIXED_SCREEN_OUT:-$REPO_ROOT/evidence/task-0063-fast-screen-test/out}"
RUN_DIR="${OSL_FIXED_SCREEN_RUN_DIR:-$REPO_ROOT/evidence/task-0063-fast-screen-test/run}"
BIN="${OSL_FIXED_SCREEN_BIN:-/mnt/d/osl-lane-targets/g/debug/osl-privacy-hub}"
WINDOW_NAME="${OSL_FIXED_SCREEN_WINDOW_NAME:-OSL Privacy}"
WAIT_SECONDS="${OSL_FIXED_SCREEN_WAIT_SECONDS:-30}"
CAPTURE_ENABLED="${OSL_FIXED_SCREEN_CAPTURE:-1}"
CAPTURE_ATTEMPTS="${OSL_FIXED_SCREEN_CAPTURE_ATTEMPTS:-20}"
IMAGE_NAME="${OSL_FIXED_SCREEN_IMAGE_NAME:-osl-fixed-screen.png}"
META_NAME="${OSL_FIXED_SCREEN_META_NAME:-osl-fixed-screen.json}"

IMAGE_PATH="$OUT_DIR/$IMAGE_NAME"
META_PATH="$OUT_DIR/$META_NAME"
XVFB_PID=""
WM_PID=""
APP_PID=""
CLEANED_UP=0

die() {
  printf 'TASK0063_ERROR %s\n' "$*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"
}

cleanup() {
  if [ "$CLEANED_UP" -eq 1 ]; then
    return
  fi
  CLEANED_UP=1
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do
    if [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
    fi
  done
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do
    if [ -n "$pid" ]; then
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
  if [ -n "$XVFB_PID" ] && kill -0 "$XVFB_PID" >/dev/null 2>&1; then
    fake_screen_alive=yes
  else
    fake_screen_alive=no
  fi
  printf 'TASK0063_CLEANUP fake_screen_alive=%s display=%s xvfb_pid=%s wm_pid=%s app_pid=%s\n' \
    "$fake_screen_alive" "$DISPLAY_ID" "${XVFB_PID:-none}" "${WM_PID:-none}" "${APP_PID:-none}"
}
trap cleanup EXIT

require_tool Xvfb
require_tool matchbox-window-manager
require_tool xdotool
require_tool xwininfo
require_tool import
require_tool identify
require_tool sha256sum
require_tool stat
require_tool python3

[ -x "$BIN" ] || die "launcher is not executable: $BIN"

rm -rf -- "$OUT_DIR" "$RUN_DIR"
mkdir -p -- "$OUT_DIR" "$RUN_DIR"

printf 'TASK0063_SWITCHES display=%s screen=%s capture=%s window=%q out=%q bin=%q\n' \
  "$DISPLAY_ID" "$SCREEN" "$CAPTURE_ENABLED" "$WINDOW_NAME" "$OUT_DIR" "$BIN"
printf 'TASK0063_BUILD_SWITCHES CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/g cargo build --locked -j1 --manifest-path apps/osl-hub/Cargo.toml --features desktop --bin osl-privacy-hub\n'
printf 'TASK0063_RUN_SWITCHES DISPLAY=%s WAYLAND_DISPLAY= GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 NO_AT_BRIDGE=1\n' "$DISPLAY_ID"

Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp &
XVFB_PID=$!
sleep 0.4
if ! kill -0 "$XVFB_PID" >/dev/null 2>&1; then
  die "Xvfb exited before the launcher could run"
fi

DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no &
WM_PID=$!
sleep 0.2

PROFILE_DIR="$RUN_DIR/profile"
mkdir -p "$PROFILE_DIR/home" "$PROFILE_DIR/config" "$PROFILE_DIR/data" "$PROFILE_DIR/cache" "$PROFILE_DIR/tmp"
DISPLAY="$DISPLAY_ID" \
WAYLAND_DISPLAY= \
GDK_BACKEND=x11 \
LIBGL_ALWAYS_SOFTWARE=1 \
WEBKIT_DISABLE_DMABUF_RENDERER=1 \
WEBKIT_DISABLE_COMPOSITING_MODE=1 \
NO_AT_BRIDGE=1 \
HOME="$PROFILE_DIR/home" \
XDG_CONFIG_HOME="$PROFILE_DIR/config" \
XDG_DATA_HOME="$PROFILE_DIR/data" \
XDG_CACHE_HOME="$PROFILE_DIR/cache" \
TMPDIR="$PROFILE_DIR/tmp" \
"$BIN" &
APP_PID=$!
printf 'TASK0063_LAUNCH display=%s xvfb_pid=%s wm_pid=%s launcher_pid=%s\n' \
  "$DISPLAY_ID" "$XVFB_PID" "$WM_PID" "$APP_PID"

WINDOW_ID=""
deadline=$((SECONDS + WAIT_SECONDS))
while [ "$SECONDS" -lt "$deadline" ]; do
  if ! kill -0 "$APP_PID" >/dev/null 2>&1; then
    die "launcher exited before creating the fixed-screen window"
  fi
  WINDOW_ID="$(DISPLAY="$DISPLAY_ID" xdotool search --name "$WINDOW_NAME" 2>/dev/null | head -n 1 || true)"
  if [ -n "$WINDOW_ID" ]; then
    break
  fi
  sleep 0.5
done
[ -n "$WINDOW_ID" ] || die "fixed-screen window not found: $WINDOW_NAME"
printf 'TASK0063_WINDOW id=%s title=%q\n' "$WINDOW_ID" "$WINDOW_NAME"

XWININFO_OUT="$(DISPLAY="$DISPLAY_ID" xwininfo -id "$WINDOW_ID")"
WIDTH="$(printf '%s\n' "$XWININFO_OUT" | awk '/Width:/ {print $2; exit}')"
HEIGHT="$(printf '%s\n' "$XWININFO_OUT" | awk '/Height:/ {print $2; exit}')"
[ -n "$WIDTH" ] && [ -n "$HEIGHT" ] || die "fixed-screen geometry could not be read"
printf 'TASK0063_GEOMETRY width=%s height=%s\n' "${WIDTH:-unknown}" "${HEIGHT:-unknown}"

image_bytes=0
image_colors=0
capture_attempt=0
if [ "$CAPTURE_ENABLED" = "1" ]; then
  capture_status=1
  while [ "$capture_attempt" -lt "$CAPTURE_ATTEMPTS" ]; do
    capture_attempt=$((capture_attempt + 1))
    DISPLAY="$DISPLAY_ID" import -window "$WINDOW_ID" "$IMAGE_PATH"
    capture_status=$?
    if [ "$capture_status" -eq 0 ] && [ -s "$IMAGE_PATH" ]; then
      image_bytes="$(stat -c %s "$IMAGE_PATH")"
      image_colors="$(identify -format '%k' "$IMAGE_PATH" 2>/dev/null || printf '0')"
      if [ "$image_bytes" -gt 500 ] && [ "$image_colors" -gt 1 ]; then
        break
      fi
    fi
    sleep 0.5
  done
else
  printf 'TASK0063_CAPTURE_SKIPPED capture=%s\n' "$CAPTURE_ENABLED"
  capture_status=64
fi
printf 'TASK0063_CAPTURE status=%s attempts=%s bytes=%s colors=%s path=%s\n' \
  "$capture_status" "$capture_attempt" "$image_bytes" "$image_colors" "$IMAGE_PATH"

image_count="$(find "$OUT_DIR" -maxdepth 1 -type f -name '*.png' | wc -l | tr -d ' ')"
if [ "$image_count" -eq 1 ] && [ -s "$IMAGE_PATH" ]; then
  image_sha256="$(sha256sum "$IMAGE_PATH" | awk '{print $1}')"
  python3 - "$META_PATH" "$DISPLAY_ID" "$SCREEN" "$CAPTURE_ENABLED" "$WINDOW_ID" "$WIDTH" "$HEIGHT" "$IMAGE_PATH" "$image_bytes" "$image_sha256" <<'PY'
import json
import sys
from pathlib import Path

meta = {
    "schema": "osl-fixed-screen-starter-v1",
    "display": sys.argv[2],
    "screen": sys.argv[3],
    "captureEnabled": sys.argv[4] == "1",
    "windowId": sys.argv[5],
    "windowWidth": int(sys.argv[6]) if sys.argv[6].isdigit() else None,
    "windowHeight": int(sys.argv[7]) if sys.argv[7].isdigit() else None,
    "imagePath": sys.argv[8],
    "imageBytes": int(sys.argv[9]),
    "imageSha256": sys.argv[10],
}
Path(sys.argv[1]).write_text(json.dumps(meta, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
fi

metadata_count="$(find "$OUT_DIR" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d ' ')"
if [ "$capture_status" -eq 0 ] && [ "$image_count" -eq 1 ] && [ "$metadata_count" -eq 1 ] && [ "$image_bytes" -gt 500 ] && [ "$image_colors" -gt 1 ]; then
  printf 'TASK0063_CAPTURE_CHECK status=ok image_count=%s metadata_count=%s bytes=%s colors=%s image=%s metadata=%s\n' \
    "$image_count" "$metadata_count" "$image_bytes" "$image_colors" "$IMAGE_PATH" "$META_PATH"
  exit 0
fi

printf 'TASK0063_CAPTURE_CHECK status=fail image_count=%s metadata_count=%s capture_status=%s bytes=%s colors=%s image=%s metadata=%s\n' \
  "$image_count" "$metadata_count" "$capture_status" "$image_bytes" "$image_colors" "$IMAGE_PATH" "$META_PATH" >&2
exit 1
