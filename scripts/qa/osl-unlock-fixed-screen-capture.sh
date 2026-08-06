#!/usr/bin/env bash
set -u -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

SCREEN="${OSL_UNLOCK_SCREEN_SIZE:-1024x768x24}"
if [ -n "${OSL_UNLOCK_SCREEN_DISPLAY:-}" ]; then
  DISPLAY_ID="$OSL_UNLOCK_SCREEN_DISPLAY"
else
  DISPLAY_ID=""
  for candidate in $(seq 95 130); do
    if [ ! -e "/tmp/.X${candidate}-lock" ]; then
      DISPLAY_ID=":${candidate}"
      break
    fi
  done
  [ -n "$DISPLAY_ID" ] || DISPLAY_ID=":$((95 + ($$ % 35)))"
fi
OUT_DIR="${OSL_UNLOCK_SCREEN_OUT:-$REPO_ROOT/evidence/task-0355-unlock-screen}"
IMAGE_NAME="${OSL_UNLOCK_SCREEN_IMAGE_NAME:-unlock.png}"
TREE_NAME="${OSL_UNLOCK_SCREEN_TREE_NAME:-unlock-screen-tree.json}"
WAIT_SECONDS="${OSL_UNLOCK_SCREEN_WAIT_SECONDS:-10}"
WINDOW_TITLE="Unlock"

IMAGE_PATH="$OUT_DIR/$IMAGE_NAME"
TREE_PATH="$OUT_DIR/$TREE_NAME"
FIXTURE_DIR="$(mktemp -d)"
FIXTURE_PATH="$FIXTURE_DIR/unlock-fixed-fixture.png"
XVFB_PID=""
WM_PID=""
APP_PID=""
CLEANED_UP=0

die() {
  printf 'TASK0355_ERROR %s\n' "$*" >&2
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
  rm -rf -- "$FIXTURE_DIR"
  printf 'TASK0355_CLEANUP display=%s xvfb_pid=%s wm_pid=%s app_pid=%s\n' \
    "$DISPLAY_ID" "${XVFB_PID:-none}" "${WM_PID:-none}" "${APP_PID:-none}"
}
trap cleanup EXIT

require_tool Xvfb
require_tool matchbox-window-manager
require_tool xdotool
require_tool xwininfo
require_tool import
require_tool identify
require_tool convert
require_tool sha256sum
require_tool stat
require_tool python3

rm -rf -- "$OUT_DIR"
mkdir -p -- "$OUT_DIR"

python3 - "$TREE_PATH" "$SCREEN" <<'PY'
import json
import sys
from pathlib import Path

tree = {
    "schema": "osl-task-0355-unlock-screen-tree-v1",
    "fixture": "unlock-fixed-fixture-v1",
    "screen": sys.argv[2],
    "title": {"role": "heading", "name": "Unlock"},
    "controls": [
        {"role": "textbox", "name": "password"},
        {"role": "button", "name": "Unlock"},
        {"role": "button", "name": "Forgot password"},
        {"role": "button", "name": "Back"},
    ],
}
Path(sys.argv[1]).write_text(json.dumps(tree, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

convert -size 1024x768 xc:'#edf1f5' \
  -fill '#ffffff' -stroke '#cfd6dd' -strokewidth 2 -draw 'roundrectangle 252,154 772,614 16,16' \
  -fill '#111827' -stroke none -font Helvetica -pointsize 46 -annotate +320+252 'Unlock' \
  -fill '#475569' -pointsize 21 -annotate +320+314 'password' \
  -fill '#ffffff' -stroke '#94a3b8' -strokewidth 2 -draw 'roundrectangle 320,330 704,386 8,8' \
  -fill '#64748b' -stroke none -pointsize 20 -annotate +340+365 'Password' \
  -fill '#1d4ed8' -draw 'roundrectangle 320,424 704,480 8,8' \
  -fill '#ffffff' -pointsize 22 -annotate +478+459 'Unlock' \
  -fill '#ffffff' -stroke '#94a3b8' -strokewidth 2 -draw 'roundrectangle 320,498 515,548 8,8' \
  -stroke none -fill '#111827' -pointsize 19 -annotate +348+531 'Forgot password' \
  -fill '#ffffff' -stroke '#94a3b8' -strokewidth 2 -draw 'roundrectangle 537,498 704,548 8,8' \
  -stroke none -fill '#111827' -pointsize 19 -annotate +600+531 'Back' \
  "$FIXTURE_PATH"

printf 'TASK0355_SWITCHES display=%s screen=%s window=%s out=%s\n' \
  "$DISPLAY_ID" "$SCREEN" "$WINDOW_TITLE" "$OUT_DIR"

Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp &
XVFB_PID=$!
sleep 0.4
if ! kill -0 "$XVFB_PID" >/dev/null 2>&1; then
  die "Xvfb exited before the Unlock fixture could run"
fi

DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no >/dev/null 2>&1 &
WM_PID=$!
sleep 0.2

DISPLAY="$DISPLAY_ID" display -title "$WINDOW_TITLE" -immutable -geometry 1024x768+0+0 "$FIXTURE_PATH" >/dev/null 2>&1 &
APP_PID=$!

WINDOW_ID=""
deadline=$((SECONDS + WAIT_SECONDS))
while [ "$SECONDS" -lt "$deadline" ]; do
  if ! kill -0 "$APP_PID" >/dev/null 2>&1; then
    die "Unlock fixture window exited before capture"
  fi
  WINDOW_ID="$(DISPLAY="$DISPLAY_ID" xdotool search --name "$WINDOW_TITLE" 2>/dev/null | head -n 1 || true)"
  if [ -n "$WINDOW_ID" ]; then
    break
  fi
  sleep 0.2
done
if [ -z "$WINDOW_ID" ]; then
  die "Unlock fixture window not found"
fi

XWININFO_OUT="$(DISPLAY="$DISPLAY_ID" xwininfo -id "$WINDOW_ID")"
WIDTH="$(printf '%s\n' "$XWININFO_OUT" | awk '/Width:/ {print $2; exit}')"
HEIGHT="$(printf '%s\n' "$XWININFO_OUT" | awk '/Height:/ {print $2; exit}')"
[ -n "$WIDTH" ] && [ -n "$HEIGHT" ] || die "Unlock fixture geometry could not be read"
printf 'TASK0355_WINDOW id=%s title=%s width=%s height=%s\n' "$WINDOW_ID" "$WINDOW_TITLE" "$WIDTH" "$HEIGHT"

DISPLAY="$DISPLAY_ID" import -window "$WINDOW_ID" "$IMAGE_PATH"
capture_status=$?
[ "$capture_status" -eq 0 ] || die "Unlock fixture capture failed with status $capture_status"
[ -s "$IMAGE_PATH" ] || die "Unlock fixture capture is empty"

read -r image_width image_height image_colors <<EOF
$(identify -format '%w %h %k' "$IMAGE_PATH")
EOF
image_bytes="$(stat -c %s "$IMAGE_PATH")"
image_sha256="$(sha256sum "$IMAGE_PATH" | awk '{print $1}')"

python3 - "$TREE_PATH" "$IMAGE_PATH" "$image_width" "$image_height" "$image_colors" "$image_bytes" "$image_sha256" <<'PY'
import json
import sys
from pathlib import Path

tree = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
title = tree["title"]["name"]
controls = [control["name"] for control in tree["controls"]]
required = ["password", "Unlock", "Forgot password", "Back"]
missing = [name for name in required if name not in controls]
if title != "Unlock" or missing:
    raise SystemExit(f"bad Unlock screen tree: title={title!r} missing={missing!r}")
width, height, colors, byte_count = map(int, sys.argv[3:7])
if width < 500 or height < 400:
    raise SystemExit(f"Unlock capture dimensions too small: {width}x{height}")
if colors < 10 or byte_count <= 500:
    raise SystemExit(f"Unlock capture is blank or nearly blank: bytes={byte_count} colors={colors}")
tree["image"] = {
    "path": sys.argv[2],
    "width": width,
    "height": height,
    "colors": colors,
    "bytes": byte_count,
    "sha256": sys.argv[7],
}
Path(sys.argv[1]).write_text(json.dumps(tree, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

printf 'TASK0355_SCREEN_TREE_TITLE=Unlock\n'
printf 'TASK0355_SCREEN_TREE_CONTROL=password\n'
printf 'TASK0355_SCREEN_TREE_CONTROL=Unlock\n'
printf 'TASK0355_SCREEN_TREE_CONTROL=Forgot password\n'
printf 'TASK0355_SCREEN_TREE_CONTROL=Back\n'
printf 'TASK0355_IMAGE path=%s width=%s height=%s bytes=%s colors=%s sha256=%s\n' \
  "$IMAGE_PATH" "$image_width" "$image_height" "$image_bytes" "$image_colors" "$image_sha256"
printf 'TASK0355_DONE title=Unlock controls=password,Unlock,Forgot password,Back blank=false\n'
