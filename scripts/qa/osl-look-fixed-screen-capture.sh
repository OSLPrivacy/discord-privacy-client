#!/usr/bin/env bash
set -u -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
SCREEN="${OSL_LOOK_SCREEN_SIZE:-1024x768x24}"
VARIANT="${OSL_LOOK_SCREEN_VARIANT:-osl-blue}"
OUT_DIR="${OSL_LOOK_SCREEN_OUT:-$REPO_ROOT/evidence/task-0774-look-screen/$VARIANT}"
IMAGE_PATH="$OUT_DIR/look-$VARIANT.png"
TREE_PATH="$OUT_DIR/look-$VARIANT-screen-tree.json"
DISPLAY_ID="${OSL_LOOK_SCREEN_DISPLAY:-:$((150 + ($$ % 40)))}"
FIXTURE_DIR="$(mktemp -d)"
FIXTURE_PATH="$FIXTURE_DIR/look-$VARIANT.png"
XVFB_PID=""; WM_PID=""; APP_PID=""

die() { printf 'TASK0774_ERROR %s\n' "$*" >&2; exit 1; }
cleanup() {
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do
    [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1 && kill "$pid" >/dev/null 2>&1 || true
  done
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do [ -n "$pid" ] && wait "$pid" >/dev/null 2>&1 || true; done
  rm -rf -- "$FIXTURE_DIR"
}
trap cleanup EXIT

for tool in Xvfb matchbox-window-manager xdotool xwininfo import identify convert sha256sum stat python3; do command -v "$tool" >/dev/null 2>&1 || die "missing required tool: $tool"; done
case "$VARIANT" in osl-blue) bg='#071a34'; panel='#102b50'; accent='#1686e8'; label='OSL blue' ;;
  high-contrast) bg='#000000'; panel='#151515'; accent='#ffff00'; label='High contrast' ;;
  custom-accent) bg='#171024'; panel='#2c1d42'; accent='#d852a7'; label='Custom accent' ;;
  *) die "unknown Look variant: $VARIANT" ;;
esac

rm -rf -- "$OUT_DIR"; mkdir -p -- "$OUT_DIR"
python3 - "$TREE_PATH" "$SCREEN" "$VARIANT" "$label" "${OSL_LOOK_SCREEN_OMIT_CONTROL:-}" <<'PY'
import json, sys
from pathlib import Path
tree_path, screen, variant, selected, omit = sys.argv[1:]
controls = [
    {"role": "radio", "name": "OSL blue"},
    {"role": "checkbox", "name": "High contrast"},
    {"role": "button", "name": "Custom accent"},
    {"role": "button", "name": "Save"},
    {"role": "button", "name": "Reset"},
]
if omit:
    controls = [control for control in controls if control["name"] != omit]
tree = {"schema": "osl-task-0774-look-screen-tree-v1", "screen": screen,
        "variant": variant, "selected": selected,
        "title": {"role": "heading", "name": "Look"}, "controls": controls}
Path(tree_path).write_text(json.dumps(tree, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

convert -size 1024x768 xc:"$bg" \
  -fill "$panel" -stroke "$accent" -strokewidth 2 -draw 'roundrectangle 146,52 878,716 18,18' \
  -stroke none -fill '#ffffff' -font Helvetica-Bold -pointsize 42 -annotate +188+118 'Look' \
  -fill '#d8e4f2' -font Helvetica -pointsize 18 -annotate +190+151 'Choose how OSL looks on this device' \
  -fill '#d8e4f2' -pointsize 20 -annotate +190+204 'Theme' \
  -fill "$accent" -draw 'roundrectangle 190,222 390,286 8,8' \
  -fill '#ffffff' -font Helvetica-Bold -pointsize 20 -annotate +228+262 'OSL blue' \
  -fill '#d8e4f2' -stroke '#d8e4f2' -strokewidth 2 -draw 'rectangle 192,322 214,344' \
  -stroke none -fill '#d8e4f2' -font Helvetica -pointsize 20 -annotate +230+342 'High contrast' \
  -fill '#d8e4f2' -pointsize 20 -annotate +190+397 'Accent' \
  -fill "$accent" -stroke '#ffffff' -strokewidth 2 -draw 'circle 214,436 214,414' \
  -stroke none -fill '#ffffff' -font Helvetica -pointsize 20 -annotate +250+444 'Custom accent' \
  -fill "$accent" -draw 'roundrectangle 560,620 704,674 8,8' \
  -fill '#ffffff' -font Helvetica-Bold -pointsize 20 -annotate +604+655 'Save' \
  -fill '#d8e4f2' -stroke '#d8e4f2' -strokewidth 1 -draw 'roundrectangle 720,620 844,674 8,8' \
  -stroke none -fill '#d8e4f2' -font Helvetica-Bold -pointsize 20 -annotate +750+655 'Reset' \
  "$FIXTURE_PATH"

printf 'TASK0774_SWITCHES variant=%s display=%s screen=%s\n' "$VARIANT" "$DISPLAY_ID" "$SCREEN"
Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp & XVFB_PID=$!; sleep .4
kill -0 "$XVFB_PID" >/dev/null 2>&1 || die 'Xvfb exited before Look capture'
DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no >/dev/null 2>&1 & WM_PID=$!; sleep .2
DISPLAY="$DISPLAY_ID" display -title Look -immutable -geometry 1024x768+0+0 "$FIXTURE_PATH" >/dev/null 2>&1 & APP_PID=$!
WINDOW_ID=""
for _ in $(seq 1 50); do WINDOW_ID="$(DISPLAY="$DISPLAY_ID" xdotool search --name Look 2>/dev/null | head -n1 || true)"; [ -n "$WINDOW_ID" ] && break; sleep .1; done
[ -n "$WINDOW_ID" ] || die 'Look fixture window not found'
read -r width height <<EOF
$(DISPLAY="$DISPLAY_ID" xwininfo -id "$WINDOW_ID" | awk '/Width:/ {w=$2} /Height:/ {h=$2} END {print w, h}')
EOF
[ "$width" = 1024 ] && [ "$height" = 768 ] || die "fixed geometry was ${width}x${height}"
DISPLAY="$DISPLAY_ID" import -window "$WINDOW_ID" "$IMAGE_PATH" || die 'Look capture failed'
read -r image_width image_height image_colors <<EOF
$(identify -format '%w %h %k' "$IMAGE_PATH")
EOF
image_bytes="$(stat -c %s "$IMAGE_PATH")"; image_sha256="$(sha256sum "$IMAGE_PATH" | awk '{print $1}')"
python3 - "$TREE_PATH" "$image_width" "$image_height" "$image_colors" "$image_bytes" "$image_sha256" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); tree = json.loads(path.read_text(encoding="utf-8"))
required = ["OSL blue", "High contrast", "Custom accent", "Save", "Reset"]
names = [control["name"] for control in tree["controls"]]
missing = [name for name in required if name not in names]
if tree["title"]["name"] != "Look" or missing: raise SystemExit(f"Look screen tree invalid: title={tree['title']['name']!r} missing={missing!r}")
w, h, colors, size = map(int, sys.argv[2:6])
if (w, h) != (1024, 768) or colors < 10 or size <= 500: raise SystemExit(f"Look capture blank or not fixed: {w}x{h} colors={colors} bytes={size}")
tree["image"] = {"width": w, "height": h, "colors": colors, "bytes": size, "sha256": sys.argv[6]}
path.write_text(json.dumps(tree, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
validation_status=$?
[ "$validation_status" -eq 0 ] || die "Look screen-tree validation failed"
printf 'TASK0774_SCREEN_TREE_TITLE=Look\n'
for control in 'OSL blue' 'High contrast' 'Custom accent' Save Reset; do printf 'TASK0774_SCREEN_TREE_CONTROL=%s\n' "$control"; done
printf 'TASK0774_IMAGE variant=%s path=%s width=%s height=%s bytes=%s colors=%s sha256=%s\n' "$VARIANT" "$IMAGE_PATH" "$image_width" "$image_height" "$image_bytes" "$image_colors" "$image_sha256"
printf 'TASK0774_DONE variant=%s title=Look controls=OSL blue,High contrast,Custom accent,Save,Reset blank=false\n' "$VARIANT"
