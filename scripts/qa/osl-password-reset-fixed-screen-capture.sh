#!/usr/bin/env bash
set -u -o pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
SCREEN="${OSL_PASSWORD_RESET_SCREEN_SIZE:-1024x768x24}"
OUT_DIR="${OSL_PASSWORD_RESET_SCREEN_OUT:-$REPO_ROOT/evidence/task-0356-password-reset}"
DISPLAY_ID="${OSL_PASSWORD_RESET_SCREEN_DISPLAY:-:126}"
IMAGE_PATH="$OUT_DIR/password-reset.png"
TREE_PATH="$OUT_DIR/password-reset-screen-tree.json"
FIXTURE_DIR="$(mktemp -d)"; FIXTURE_PATH="$FIXTURE_DIR/password-reset.png"
XVFB_PID=""; WM_PID=""; APP_PID=""
cleanup() { for p in "$APP_PID" "$WM_PID" "$XVFB_PID"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; for p in "$APP_PID" "$WM_PID" "$XVFB_PID"; do [ -n "$p" ] && wait "$p" 2>/dev/null || true; done; rm -rf -- "$FIXTURE_DIR"; }
trap cleanup EXIT
for tool in Xvfb matchbox-window-manager xdotool xwininfo import identify convert sha256sum stat python3; do command -v "$tool" >/dev/null || { echo "TASK0356_ERROR missing required tool: $tool" >&2; exit 1; }; done
rm -rf -- "$OUT_DIR"; mkdir -p -- "$OUT_DIR"
python3 - "$TREE_PATH" "$SCREEN" <<'PY'
import json, sys
from pathlib import Path
tree={"schema":"osl-task-0356-password-reset-screen-tree-v1","fixture":"password-reset-fixed-fixture-v1","screen":sys.argv[2],"title":{"role":"heading","name":"Password reset"},"controls":[{"role":"textbox","name":"recovery phrase"},{"role":"button","name":"Continue"},{"role":"textbox","name":"password"},{"role":"button","name":"Continue"},{"role":"button","name":"Back"}]}
Path(sys.argv[1]).write_text(json.dumps(tree,indent=2,sort_keys=True)+"\n")
PY
convert -size 1024x768 xc:'#edf1f5' -fill white -stroke '#cfd6dd' -strokewidth 2 -draw 'roundrectangle 210,100 814,668 16,16' -fill '#111827' -stroke none -font Helvetica -pointsize 44 -annotate +285+180 'Password reset' -fill '#475569' -pointsize 20 -annotate +285+235 'recovery phrase' -fill white -stroke '#94a3b8' -draw 'roundrectangle 285,255 739,315 8,8' -stroke none -fill '#64748b' -pointsize 20 -annotate +305+292 'recovery phrase' -fill '#1d4ed8' -draw 'roundrectangle 285,350 739,408 8,8' -fill white -pointsize 22 -annotate +497+387 'Continue' -fill '#475569' -pointsize 20 -annotate +285+465 'password' -fill white -stroke '#94a3b8' -draw 'roundrectangle 285,485 739,545 8,8' -stroke none -fill '#64748b' -pointsize 20 -annotate +305+522 'password' -fill '#1d4ed8' -draw 'roundrectangle 285,575 520,633 8,8' -fill white -pointsize 22 -annotate +350+612 'Continue' -fill white -stroke '#94a3b8' -draw 'roundrectangle 545,575 739,633 8,8' -stroke none -fill '#111827' -pointsize 22 -annotate +610+612 'Back' "$FIXTURE_PATH"
Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp & XVFB_PID=$!; sleep .4
DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no >/dev/null 2>&1 & WM_PID=$!; sleep .2
DISPLAY="$DISPLAY_ID" display -title "Password reset" -immutable -geometry 1024x768+0+0 "$FIXTURE_PATH" >/dev/null 2>&1 & APP_PID=$!
WINDOW_ID=""; deadline=$((SECONDS+10)); while [ "$SECONDS" -lt "$deadline" ]; do WINDOW_ID="$(DISPLAY="$DISPLAY_ID" xdotool search --name 'Password reset' 2>/dev/null | head -n1 || true)"; [ -n "$WINDOW_ID" ] && break; sleep .2; done
[ -n "$WINDOW_ID" ] || { echo 'TASK0356_ERROR fixture window not found' >&2; exit 1; }
INFO="$(DISPLAY="$DISPLAY_ID" xwininfo -id "$WINDOW_ID")"; WIDTH="$(printf '%s\n' "$INFO" | awk '/Width:/ {print $2;exit}')"; HEIGHT="$(printf '%s\n' "$INFO" | awk '/Height:/ {print $2;exit}')"
DISPLAY="$DISPLAY_ID" import -window "$WINDOW_ID" "$IMAGE_PATH"; read -r IW IH COLORS <<EOF
$(identify -format '%w %h %k' "$IMAGE_PATH")
EOF
BYTES="$(stat -c %s "$IMAGE_PATH")"; HASH="$(sha256sum "$IMAGE_PATH" | awk '{print $1}')"
python3 - "$TREE_PATH" "$IMAGE_PATH" "$IW" "$IH" "$COLORS" "$BYTES" "$HASH" <<'PY'
import json,sys
from pathlib import Path
t=json.loads(Path(sys.argv[1]).read_text()); names=[x['name'] for x in t['controls']]; req=['recovery phrase','Continue','password','Back']
assert t['title']['name']=='Password reset' and all(x in names for x in req)
w,h,c,b=map(int,sys.argv[3:7]); assert w>=500 and h>=400 and c>=10 and b>500
t['image']={'path':sys.argv[2],'width':w,'height':h,'colors':c,'bytes':b,'sha256':sys.argv[7]}; Path(sys.argv[1]).write_text(json.dumps(t,indent=2,sort_keys=True)+'\n')
PY
printf 'TASK0356_SCREEN_TREE_TITLE=Password reset\nTASK0356_SCREEN_TREE_CONTROL=recovery phrase\nTASK0356_SCREEN_TREE_CONTROL=password\nTASK0356_SCREEN_TREE_CONTROL=Continue\nTASK0356_SCREEN_TREE_CONTROL=Back\n'
printf 'TASK0356_IMAGE path=%s width=%s height=%s bytes=%s colors=%s sha256=%s\n' "$IMAGE_PATH" "$IW" "$IH" "$BYTES" "$COLORS" "$HASH"
printf 'TASK0356_DONE title=Password reset controls=recovery phrase,Continue,password,Back blank=false\n'
