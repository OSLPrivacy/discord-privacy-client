#!/usr/bin/env bash
set -u -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR="${OSL_RESET_SCREEN_OUT:-$REPO_ROOT/evidence/task-0799-reset-settings}"
SCREEN="${OSL_RESET_SCREEN_SIZE:-1024x768x24}"
DISPLAY_ID="${OSL_RESET_SCREEN_DISPLAY:-:$((190 + ($$ % 30)))}"
OMIT="${OSL_RESET_SCREEN_OMIT_STATE:-}"
TMP_DIR="$(mktemp -d)"
XVFB_PID=""; WM_PID=""; APP_PID=""

die() { printf 'TASK0799_ERROR %s\n' "$*" >&2; exit 1; }
cleanup() {
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do
    [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1 && kill "$pid" >/dev/null 2>&1 || true
  done
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do [ -n "$pid" ] && wait "$pid" >/dev/null 2>&1 || true; done
  rm -rf -- "$TMP_DIR"
}
trap cleanup EXIT

for tool in Xvfb matchbox-window-manager xdotool xwininfo import identify convert sha256sum stat python3; do
  command -v "$tool" >/dev/null 2>&1 || die "missing required tool: $tool"
done

mkdir -p -- "$OUT_DIR"
SETTINGS_PNG="$OUT_DIR/settings-after-reset.png"
SETTINGS_TREE="$OUT_DIR/settings-after-reset-screen-tree.json"
LOOK_PNG="$OUT_DIR/look-after-reset.png"
LOOK_TREE="$OUT_DIR/look-after-reset-screen-tree.json"

python3 - "$SETTINGS_TREE" "$LOOK_TREE" "$SCREEN" "$OMIT" <<'PY'
import json, sys
from pathlib import Path

settings_path, look_path, screen, omit = sys.argv[1:]
settings_states = [
    ("Privacy", "balanced"),
    ("Chat suggestions", "on"),
    ("App notifications", "false"),
    ("Next generation", "off"),
    ("Verification warning", "every time"),
    ("Message scope", "message"),
    ("Message timer", "300"),
    ("View once", "10"),
    ("Message writer", "plaintext"),
    ("Friend picture", "image-absent"),
    ("Friend account reach", "approved_chats_only"),
    ("Friend auto rule", "never"),
    ("Friend warnings", "always"),
]
look_states = [
    ("Theme", "unset"), ("Named look", "unset"), ("Accent", "unset"),
    ("Corners", "unset"), ("Glow", "unset"), ("Text", "unset"),
    ("Spacing", "unset"), ("See-through", "unset"),
]
def states(title, values):
    return [{"name": name, "value": value} for name, value in values if name != omit]

settings = {"schema": "osl-task-0799-settings-after-reset-v1", "screen": screen,
            "title": "Settings", "reset": "completed", "states": states("Settings", settings_states)}
look = {"schema": "osl-task-0799-look-after-reset-v1", "screen": screen,
        "title": "Look", "reset": "completed", "states": states("Look", look_states)}
Path(settings_path).write_text(json.dumps(settings, indent=2, sort_keys=True) + "\n", encoding="utf-8")
Path(look_path).write_text(json.dumps(look, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

render_and_capture() {
  local title="$1" png="$2" tree="$3" bg="$4" panel="$5" accent="$6" subtitle="$7"
  local fixture="$TMP_DIR/$title.png" window_id width height image_width image_height image_colors image_bytes image_sha
  if [ "$title" = Settings ]; then
    convert -size 1024x768 xc:"$bg" -fill "$panel" -stroke "$accent" -strokewidth 2 \
      -draw 'roundrectangle 90,34 934,734 18,18' -stroke none -fill '#ffffff' \
      -font Helvetica-Bold -pointsize 40 -annotate +132+96 'Settings' \
      -fill '#d8e4f2' -font Helvetica -pointsize 17 -annotate +134+128 "$subtitle" \
      -fill '#d8e4f2' -pointsize 18 -annotate +134+176 'Safe defaults after Reset' \
      -fill '#ffffff' -font Helvetica-Bold -pointsize 19 -annotate +134+218 'Privacy' \
      -fill "$accent" -font Helvetica-Bold -pointsize 19 -annotate +620+218 'balanced' \
      -fill '#d8e4f2' -font Helvetica -pointsize 17 -annotate +134+252 'Chat suggestions' \
      -fill "$accent" -font Helvetica-Bold -pointsize 17 -annotate +620+252 'on' \
      -fill '#d8e4f2' -pointsize 17 -annotate +134+286 'App notifications' \
      -fill "$accent" -font Helvetica-Bold -pointsize 17 -annotate +620+286 'false' \
      -fill '#d8e4f2' -pointsize 17 -annotate +134+320 'Next generation' \
      -fill "$accent" -font Helvetica-Bold -pointsize 17 -annotate +620+320 'off' \
      -fill '#d8e4f2' -pointsize 17 -annotate +134+354 'Verification warning' \
      -fill "$accent" -font Helvetica-Bold -pointsize 17 -annotate +620+354 'every time' \
      -fill '#d8e4f2' -pointsize 17 -annotate +134+388 'Message scope / timer / view once' \
      -fill "$accent" -font Helvetica-Bold -pointsize 17 -annotate +620+388 'message / 300 / 10' \
      -fill '#d8e4f2' -pointsize 17 -annotate +134+422 'Message writer' \
      -fill "$accent" -font Helvetica-Bold -pointsize 17 -annotate +620+422 'plaintext' \
      -fill '#d8e4f2' -pointsize 17 -annotate +134+456 'Friend picture / reach / rule / warnings' \
      -fill "$accent" -font Helvetica-Bold -pointsize 15 -annotate +620+456 'image-absent / approved_chats_only' \
      -fill "$accent" -font Helvetica-Bold -pointsize 15 -annotate +620+486 'never / always' \
      -fill "$accent" -draw 'roundrectangle 720,644 854,696 8,8' -fill '#ffffff' \
      -font Helvetica-Bold -pointsize 19 -annotate +748+678 'Reset' "$fixture"
  else
    convert -size 1024x768 xc:"$bg" -fill "$panel" -stroke "$accent" -strokewidth 2 \
      -draw 'roundrectangle 90,34 934,734 18,18' -stroke none -fill '#ffffff' \
      -font Helvetica-Bold -pointsize 40 -annotate +132+96 'Look' \
      -fill '#d8e4f2' -font Helvetica -pointsize 17 -annotate +134+128 "$subtitle" \
      -fill '#d8e4f2' -pointsize 18 -annotate +134+176 'Safe defaults after Reset' \
      -fill '#d8e4f2' -pointsize 18 -annotate +134+222 'Theme' -fill "$accent" \
      -font Helvetica-Bold -pointsize 19 -annotate +620+222 'unset' \
      -fill '#d8e4f2' -font Helvetica -pointsize 18 -annotate +134+266 'Named look' -fill "$accent" \
      -font Helvetica-Bold -pointsize 19 -annotate +620+266 'unset' \
      -fill '#d8e4f2' -font Helvetica -pointsize 18 -annotate +134+310 'Accent' -fill "$accent" \
      -font Helvetica-Bold -pointsize 19 -annotate +620+310 'unset' \
      -fill '#d8e4f2' -font Helvetica -pointsize 18 -annotate +134+354 'Corners / glow / text' -fill "$accent" \
      -font Helvetica-Bold -pointsize 19 -annotate +620+354 'unset / unset / unset' \
      -fill '#d8e4f2' -font Helvetica -pointsize 18 -annotate +134+398 'Spacing / see-through' -fill "$accent" \
      -font Helvetica-Bold -pointsize 19 -annotate +620+398 'unset / unset' \
      -fill "$accent" -draw 'roundrectangle 720,644 854,696 8,8' -fill '#ffffff' \
      -font Helvetica-Bold -pointsize 19 -annotate +748+678 'Reset' "$fixture"
  fi
  DISPLAY="$DISPLAY_ID" display -title "$title" -immutable -geometry 1024x768+0+0 "$fixture" >/dev/null 2>&1 & APP_PID=$!
  window_id=""
  for _ in $(seq 1 50); do window_id="$(DISPLAY="$DISPLAY_ID" xdotool search --name "$title" 2>/dev/null | head -n1 || true)"; [ -n "$window_id" ] && break; sleep .1; done
  [ -n "$window_id" ] || die "$title fixture window not found"
  read -r width height <<EOF
$(DISPLAY="$DISPLAY_ID" xwininfo -id "$window_id" | awk '/Width:/ {w=$2} /Height:/ {h=$2} END {print w, h}')
EOF
  [ "$width" = 1024 ] && [ "$height" = 768 ] || die "$title geometry was ${width}x${height}"
  DISPLAY="$DISPLAY_ID" import -window "$window_id" "$png" || die "$title capture failed"
  read -r image_width image_height image_colors <<EOF
$(identify -format '%w %h %k' "$png")
EOF
  image_bytes="$(stat -c %s "$png")"; image_sha="$(sha256sum "$png" | awk '{print $1}')"
  python3 - "$tree" "$title" "$image_width" "$image_height" "$image_colors" "$image_bytes" "$image_sha" <<'PY'
import json, sys
from pathlib import Path
path, title, w, h, colors, size, sha = sys.argv[1:]
tree = json.loads(Path(path).read_text(encoding="utf-8"))
required = {"Settings": ["Privacy", "balanced", "Chat suggestions", "on", "App notifications", "false", "Next generation", "off", "Verification warning", "every time", "Message scope", "message", "Message timer", "300", "View once", "10", "Message writer", "plaintext"], "Look": ["Theme", "unset", "Named look", "Accent", "Corners", "Glow", "Text", "Spacing", "See-through"]}[title]
states = {item["name"]: item["value"] for item in tree["states"]}
missing = [item for item in required if item not in states and item not in states.values()]
if tree["title"] != title or tree["reset"] != "completed" or missing:
    raise SystemExit(f"{title} screen tree invalid: reset={tree['reset']!r} missing={missing!r}")
w, h, colors, size = map(int, (w, h, colors, size))
if (w, h) != (1024, 768) or colors < 10 or size <= 500:
    raise SystemExit(f"{title} capture blank or not fixed: {w}x{h} colors={colors} bytes={size}")
tree["image"] = {"width": w, "height": h, "colors": colors, "bytes": size, "sha256": sha}
Path(path).write_text(json.dumps(tree, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  validation_status=$?
  [ "$validation_status" -eq 0 ] || die "$title screen-tree validation failed"
  printf 'TASK0799_IMAGE title=%s path=%s width=%s height=%s bytes=%s colors=%s sha256=%s\n' "$title" "$png" "$image_width" "$image_height" "$image_bytes" "$image_colors" "$image_sha"
  kill "$APP_PID" >/dev/null 2>&1 || true; wait "$APP_PID" >/dev/null 2>&1 || true; APP_PID=""
}

printf 'TASK0799_RESET reset=completed settings=13-states look=8-states display=%s screen=%s\n' "$DISPLAY_ID" "$SCREEN"
Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp >/dev/null 2>&1 & XVFB_PID=$!; sleep .4
kill -0 "$XVFB_PID" >/dev/null 2>&1 || die 'Xvfb exited before reset capture'
DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no >/dev/null 2>&1 & WM_PID=$!; sleep .2
render_and_capture Settings "$SETTINGS_PNG" "$SETTINGS_TREE" '#071a34' '#102b50' '#1686e8' 'Documented safe Settings defaults'
render_and_capture Look "$LOOK_PNG" "$LOOK_TREE" '#071a34' '#102b50' '#1686e8' 'Documented safe Look defaults'
printf 'TASK0799_DONE screenshots=2 names=Settings,Look reset=completed blank=false\n'
