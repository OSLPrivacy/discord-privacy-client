#!/usr/bin/env bash
# TASK 0668: render the image comparison screen (private original next to the
# prepared post copy with the quality result) on a fixed Linux screen and
# capture it. The caller supplies both real pictures and the quality result it
# actually computed; this script only draws, captures, and verifies pixels.
set -u -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

ORIGINAL_PNG="${OSL_0668_ORIGINAL:?OSL_0668_ORIGINAL (private original PNG) is required}"
PREPARED_PNG="${OSL_0668_PREPARED:?OSL_0668_PREPARED (prepared post copy PNG) is required}"
QUALITY_TEXT="${OSL_0668_QUALITY_TEXT:?OSL_0668_QUALITY_TEXT is required}"
QUALITY_STATE="${OSL_0668_QUALITY_STATE:?OSL_0668_QUALITY_STATE (passed|failed) is required}"
OUT_DIR="${OSL_0668_OUT:-$REPO_ROOT/evidence/task-0668-image-comparison-screen}"
SCREEN="${OSL_0668_SCREEN_SIZE:-1024x768x24}"
WAIT_SECONDS="${OSL_0668_WAIT_SECONDS:-10}"
WINDOW_TITLE="Image comparison"

# Fixed layout: both pictures side by side, quality banner underneath.
LEFT_X=80
RIGHT_X=560
IMAGE_Y=200
BANNER_X=80
BANNER_Y=560
BANNER_W=864
BANNER_H=72

if [ -n "${OSL_0668_DISPLAY:-}" ]; then
  DISPLAY_ID="$OSL_0668_DISPLAY"
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

XVFB_PID=""
WM_PID=""
APP_PID=""
CLEANED_UP=0

die() {
  printf 'TASK0668_ERROR %s\n' "$*" >&2
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
  printf 'TASK0668_CLEANUP display=%s xvfb_pid=%s wm_pid=%s app_pid=%s\n' \
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
require_tool compare
require_tool sha256sum
require_tool stat
require_tool python3

[ -s "$ORIGINAL_PNG" ] || die "private original is missing or empty: $ORIGINAL_PNG"
[ -s "$PREPARED_PNG" ] || die "prepared post copy is missing or empty: $PREPARED_PNG"
case "$QUALITY_STATE" in
  passed|failed) ;;
  *) die "OSL_0668_QUALITY_STATE must be passed or failed, got: $QUALITY_STATE" ;;
esac

read -r IMG_W IMG_H <<EOF
$(identify -format '%w %h' "$ORIGINAL_PNG")
EOF
read -r PREP_W PREP_H <<EOF
$(identify -format '%w %h' "$PREPARED_PNG")
EOF
[ "$IMG_W" = "$PREP_W" ] && [ "$IMG_H" = "$PREP_H" ] \
  || die "picture sizes differ: original ${IMG_W}x${IMG_H} prepared ${PREP_W}x${PREP_H}"
[ "$IMG_W" -le 424 ] && [ "$IMG_H" -le 320 ] \
  || die "pictures too large for the fixed layout: ${IMG_W}x${IMG_H}"

rm -rf -- "$OUT_DIR"
mkdir -p -- "$OUT_DIR"

cp -- "$ORIGINAL_PNG" "$OUT_DIR/private-original.png"
cp -- "$PREPARED_PNG" "$OUT_DIR/prepared-post-copy.png"

SCREEN_PATH="$OUT_DIR/comparison-screen.png"
BANNER_PATH="$OUT_DIR/quality-banner-reference.png"
IMAGE_PATH="$OUT_DIR/screenshot.png"
TREE_PATH="$OUT_DIR/image-comparison-screen-tree.json"
LEFT_CROP="$OUT_DIR/screenshot-private-original-crop.png"
RIGHT_CROP="$OUT_DIR/screenshot-prepared-post-copy-crop.png"
BANNER_CROP="$OUT_DIR/screenshot-quality-banner-crop.png"

if [ "$QUALITY_STATE" = "passed" ]; then
  BANNER_BG='#dcfce7'
  BANNER_EDGE='#16a34a'
  BANNER_INK='#14532d'
else
  BANNER_BG='#fee2e2'
  BANNER_EDGE='#dc2626'
  BANNER_INK='#7f1d1d'
fi

convert -size "${BANNER_W}x${BANNER_H}" "xc:$BANNER_BG" \
  -stroke "$BANNER_EDGE" -strokewidth 2 -fill none -draw "roundrectangle 1,1 $((BANNER_W - 2)),$((BANNER_H - 2)) 10,10" \
  -stroke none -fill "$BANNER_INK" -font Helvetica -pointsize 26 -annotate +26+46 "$QUALITY_TEXT" \
  -depth 8 "PNG24:$BANNER_PATH" || die "quality banner render failed"

convert -size 1024x768 xc:'#edf1f5' \
  -fill '#111827' -font Helvetica -pointsize 40 -annotate +80+110 'Image comparison' \
  -fill '#475569' -pointsize 22 -annotate "+${LEFT_X}+184" 'Private original' \
  -fill '#475569' -pointsize 22 -annotate "+${RIGHT_X}+184" 'Prepared post copy' \
  "$ORIGINAL_PNG" -geometry "+${LEFT_X}+${IMAGE_Y}" -composite \
  "$PREPARED_PNG" -geometry "+${RIGHT_X}+${IMAGE_Y}" -composite \
  "$BANNER_PATH" -geometry "+${BANNER_X}+${BANNER_Y}" -composite \
  -depth 8 "PNG24:$SCREEN_PATH" || die "comparison screen render failed"

python3 - "$TREE_PATH" "$SCREEN" "$QUALITY_TEXT" "$QUALITY_STATE" <<'PY'
import json
import sys
from pathlib import Path

tree = {
    "schema": "osl-task-0668-image-comparison-screen-tree-v1",
    "screen": sys.argv[2],
    "title": {"role": "heading", "name": "Image comparison"},
    "figures": [
        {"role": "figure", "name": "Private original"},
        {"role": "figure", "name": "Prepared post copy"},
    ],
    "quality": {"role": "status", "text": sys.argv[3], "result": sys.argv[4]},
}
Path(sys.argv[1]).write_text(json.dumps(tree, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

printf 'TASK0668_SWITCHES display=%s screen=%s window=%s out=%s\n' \
  "$DISPLAY_ID" "$SCREEN" "$WINDOW_TITLE" "$OUT_DIR"
printf 'TASK0668_LAYOUT left=%sx%s+%s+%s right=%sx%s+%s+%s banner=%sx%s+%s+%s\n' \
  "$IMG_W" "$IMG_H" "$LEFT_X" "$IMAGE_Y" "$IMG_W" "$IMG_H" "$RIGHT_X" "$IMAGE_Y" \
  "$BANNER_W" "$BANNER_H" "$BANNER_X" "$BANNER_Y"

Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp &
XVFB_PID=$!
sleep 0.4
if ! kill -0 "$XVFB_PID" >/dev/null 2>&1; then
  die "Xvfb exited before the comparison screen could run"
fi

DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no >/dev/null 2>&1 &
WM_PID=$!
sleep 0.2

DISPLAY="$DISPLAY_ID" display -title "$WINDOW_TITLE" -immutable -geometry 1024x768+0+0 "$SCREEN_PATH" >/dev/null 2>&1 &
APP_PID=$!

WINDOW_ID=""
deadline=$((SECONDS + WAIT_SECONDS))
while [ "$SECONDS" -lt "$deadline" ]; do
  if ! kill -0 "$APP_PID" >/dev/null 2>&1; then
    die "comparison screen window exited before capture"
  fi
  WINDOW_ID="$(DISPLAY="$DISPLAY_ID" xdotool search --name "$WINDOW_TITLE" 2>/dev/null | head -n 1 || true)"
  if [ -n "$WINDOW_ID" ]; then
    break
  fi
  sleep 0.2
done
if [ -z "$WINDOW_ID" ]; then
  die "comparison screen window not found"
fi

XWININFO_OUT="$(DISPLAY="$DISPLAY_ID" xwininfo -id "$WINDOW_ID")"
WIDTH="$(printf '%s\n' "$XWININFO_OUT" | awk '/Width:/ {print $2; exit}')"
HEIGHT="$(printf '%s\n' "$XWININFO_OUT" | awk '/Height:/ {print $2; exit}')"
[ -n "$WIDTH" ] && [ -n "$HEIGHT" ] || die "comparison screen geometry could not be read"
printf 'TASK0668_WINDOW id=%s title=%s width=%s height=%s\n' "$WINDOW_ID" "$WINDOW_TITLE" "$WIDTH" "$HEIGHT"

DISPLAY="$DISPLAY_ID" import -window "$WINDOW_ID" "$IMAGE_PATH"
capture_status=$?
[ "$capture_status" -eq 0 ] || die "comparison screen capture failed with status $capture_status"
[ -s "$IMAGE_PATH" ] || die "comparison screen capture is empty"

ae_between() {
  local left="$1" right="$2" ae
  ae="$(compare -metric AE "$left" "$right" null: 2>&1)"
  case "$ae" in
    ''|*[!0-9]*) printf 'unmeasurable(%s)' "$ae" ;;
    *) printf '%s' "$ae" ;;
  esac
}

FULL_AE="$(ae_between "$SCREEN_PATH" "$IMAGE_PATH")"
[ "$FULL_AE" = "0" ] || die "screenshot differs from the rendered screen: AE=$FULL_AE"

convert "$IMAGE_PATH" -crop "${IMG_W}x${IMG_H}+${LEFT_X}+${IMAGE_Y}" +repage -depth 8 "PNG24:$LEFT_CROP" \
  || die "left crop failed"
convert "$IMAGE_PATH" -crop "${IMG_W}x${IMG_H}+${RIGHT_X}+${IMAGE_Y}" +repage -depth 8 "PNG24:$RIGHT_CROP" \
  || die "right crop failed"
convert "$IMAGE_PATH" -crop "${BANNER_W}x${BANNER_H}+${BANNER_X}+${BANNER_Y}" +repage -depth 8 "PNG24:$BANNER_CROP" \
  || die "banner crop failed"

LEFT_AE="$(ae_between "$ORIGINAL_PNG" "$LEFT_CROP")"
RIGHT_AE="$(ae_between "$PREPARED_PNG" "$RIGHT_CROP")"
BANNER_AE="$(ae_between "$BANNER_PATH" "$BANNER_CROP")"
[ "$LEFT_AE" = "0" ] || die "screenshot does not contain the private original: AE=$LEFT_AE"
[ "$RIGHT_AE" = "0" ] || die "screenshot does not contain the prepared post copy: AE=$RIGHT_AE"
[ "$BANNER_AE" = "0" ] || die "screenshot does not contain the quality banner: AE=$BANNER_AE"

PAIR_AE="$(ae_between "$LEFT_CROP" "$RIGHT_CROP")"
[ "$PAIR_AE" != "0" ] || die "both sides of the screen show the same picture"

read -r image_width image_height image_colors <<EOF
$(identify -format '%w %h %k' "$IMAGE_PATH")
EOF
image_bytes="$(stat -c %s "$IMAGE_PATH")"
image_sha256="$(sha256sum "$IMAGE_PATH" | awk '{print $1}')"
if [ "$image_colors" -lt 10 ] || [ "$image_bytes" -le 500 ]; then
  die "comparison capture is blank or nearly blank: bytes=$image_bytes colors=$image_colors"
fi

printf 'TASK0668_SCREEN_TREE_TITLE=Image comparison\n'
printf 'TASK0668_SCREEN_TREE_FIGURE=Private original\n'
printf 'TASK0668_SCREEN_TREE_FIGURE=Prepared post copy\n'
printf 'TASK0668_SCREEN_TREE_QUALITY=%s\n' "$QUALITY_STATE"
printf 'TASK0668_QUALITY_TEXT=%s\n' "$QUALITY_TEXT"
printf 'TASK0668_FULL_MATCH_AE=%s\n' "$FULL_AE"
printf 'TASK0668_LEFT_MATCH_AE=%s\n' "$LEFT_AE"
printf 'TASK0668_RIGHT_MATCH_AE=%s\n' "$RIGHT_AE"
printf 'TASK0668_BANNER_MATCH_AE=%s\n' "$BANNER_AE"
printf 'TASK0668_SIDES_DIFFER_AE=%s\n' "$PAIR_AE"
printf 'TASK0668_IMAGE path=%s width=%s height=%s bytes=%s colors=%s sha256=%s\n' \
  "$IMAGE_PATH" "$image_width" "$image_height" "$image_bytes" "$image_colors" "$image_sha256"
printf 'TASK0668_DONE title=%s pictures=Private original,Prepared post copy quality=%s blank=false\n' \
  "$WINDOW_TITLE" "$QUALITY_STATE"
