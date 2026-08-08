#!/usr/bin/env bash
set -u -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

DISPLAY_ID="${OSL_TOR_4902_DISPLAY:-:$((90 + ($$ % 30)))}"
SCREEN="${OSL_TOR_4902_SCREEN:-1440x900x24}"
OUT_DIR="${OSL_TOR_4902_OUT:-$REPO_ROOT/evidence/task-4902-tor-startup-paint/out}"
RUN_DIR="${OSL_TOR_4902_RUN_DIR:-$REPO_ROOT/evidence/task-4902-tor-startup-paint/run}"
BIN="${OSL_TOR_4902_BIN:-${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/d}/debug/osl-privacy-hub}"
WINDOW_NAME="${OSL_TOR_4902_WINDOW_NAME:-OSL Privacy}"
PAINT_DEADLINE_SECONDS="${OSL_TOR_4902_PAINT_DEADLINE_SECONDS:-3}"
DISABLE_TOR_STARTER="${OSL_TOR_4902_DISABLE_TOR_STARTER:-0}"

XVFB_PID=""
WM_PID=""
APP_PID=""
CLEANED_UP=0

die() {
  printf 'TOR-4902-ERROR %s\n' "$*" >&2
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
  printf 'TOR_4902_CLEANUP display=%s xvfb_pid=%s wm_pid=%s app_pid=%s\n' \
    "$DISPLAY_ID" "${XVFB_PID:-none}" "${WM_PID:-none}" "${APP_PID:-none}"
}
trap cleanup EXIT

require_tool Xvfb
require_tool matchbox-window-manager
require_tool xdotool
require_tool xwininfo
require_tool import
require_tool identify
require_tool python3
require_tool stat

[ -x "$BIN" ] || die "app binary is not executable: $BIN"

rm -rf -- "$OUT_DIR" "$RUN_DIR"
mkdir -p -- "$OUT_DIR" "$RUN_DIR"

PROFILE_DIR="$RUN_DIR/profile"
mkdir -p \
  "$PROFILE_DIR/home" \
  "$PROFILE_DIR/config/org.oslprivacy.hub" \
  "$PROFILE_DIR/data" \
  "$PROFILE_DIR/cache" \
  "$PROFILE_DIR/tmp"

cat >"$PROFILE_DIR/config/org.oslprivacy.hub/tor-preference.json" <<'JSON'
{"version":1,"preference":"tor"}
JSON

SLOW_ARTI="$RUN_DIR/tor-4902-slow-arti.sh"
cat >"$SLOW_ARTI" <<'SH'
#!/usr/bin/env bash
trap 'exit 0' TERM INT
sleep 45
printf 'TOR-4902-SLOW-ARTI-LATE\n'
while true; do sleep 1; done
SH
chmod 755 "$SLOW_ARTI"

printf 'TOR_4902_SWITCHES display=%s screen=%s deadline_s=%s disabled_starter=%s bin=%q out=%q\n' \
  "$DISPLAY_ID" "$SCREEN" "$PAINT_DEADLINE_SECONDS" "$DISABLE_TOR_STARTER" "$BIN" "$OUT_DIR"
printf 'TOR_4902_PROFILE=%s\n' "$PROFILE_DIR"
printf 'TOR_4902_PREFERENCE_FILE=%s\n' "$PROFILE_DIR/config/org.oslprivacy.hub/tor-preference.json"
printf 'TOR_4902_SLOW_STARTER sleep_seconds=45 path=%s disabled=%s\n' \
  "$SLOW_ARTI" "$DISABLE_TOR_STARTER"

Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp &
XVFB_PID=$!
sleep 0.4
if ! kill -0 "$XVFB_PID" >/dev/null 2>&1; then
  die "Xvfb exited before the app could run"
fi

DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no &
WM_PID=$!
sleep 0.2

app_env=(
  DISPLAY="$DISPLAY_ID"
  WAYLAND_DISPLAY=
  GDK_BACKEND=x11
  LIBGL_ALWAYS_SOFTWARE=1
  WEBKIT_DISABLE_DMABUF_RENDERER=1
  WEBKIT_DISABLE_COMPOSITING_MODE=1
  NO_AT_BRIDGE=1
  HOME="$PROFILE_DIR/home"
  XDG_CONFIG_HOME="$PROFILE_DIR/config"
  XDG_DATA_HOME="$PROFILE_DIR/data"
  XDG_CACHE_HOME="$PROFILE_DIR/cache"
  TMPDIR="$PROFILE_DIR/tmp"
)
if [ "$DISABLE_TOR_STARTER" != "1" ]; then
  app_env+=(OSL_ARTI_PROXY_PATH="$SLOW_ARTI")
fi

env "${app_env[@]}" "$BIN" >"$OUT_DIR/app.stdout" 2>"$OUT_DIR/app.stderr" &
APP_PID=$!
printf 'TOR_4902_LAUNCH pid=%s\n' "$APP_PID"

painted_windows=0
first_paint_ms=""
app_exited_before_deadline=0
deadline_ms=$((PAINT_DEADLINE_SECONDS * 1000))
start_ms="$(python3 - <<'PY'
import time
print(time.monotonic_ns() // 1_000_000)
PY
)"

while true; do
  now_ms="$(python3 - <<'PY'
import time
print(time.monotonic_ns() // 1_000_000)
PY
)"
  elapsed_ms=$((now_ms - start_ms))
  if [ "$elapsed_ms" -gt "$deadline_ms" ]; then
    break
  fi
  if ! kill -0 "$APP_PID" >/dev/null 2>&1; then
    printf 'TOR_4902_APP_EXITED_BEFORE_SECOND_3=1\n'
    app_exited_before_deadline=1
    break
  fi
  ids="$(DISPLAY="$DISPLAY_ID" xdotool search --name "$WINDOW_NAME" 2>/dev/null || true)"
  for window_id in $ids; do
    info="$(DISPLAY="$DISPLAY_ID" xwininfo -id "$window_id" 2>/dev/null || true)"
    printf '%s\n' "$info" | grep -Fq 'Map State: IsViewable' || continue
    image="$OUT_DIR/window-${window_id}.png"
    if DISPLAY="$DISPLAY_ID" import -window "$window_id" "$image" >/dev/null 2>&1 && [ -s "$image" ]; then
      bytes="$(stat -c %s "$image" 2>/dev/null || printf '0')"
      colors="$(identify -format '%k' "$image" 2>/dev/null || printf '0')"
      if [ "${bytes:-0}" -gt 500 ] && [ "${colors:-0}" -gt 1 ]; then
        painted_windows=1
        first_paint_ms="$elapsed_ms"
        printf 'TOR_4902_FIRST_PAINT_MS=%s\n' "$first_paint_ms"
        break 2
      fi
    fi
  done
  sleep 0.1
done

printf 'TOR_4902_PAINTED_WINDOWS_BY_SECOND_3=%s\n' "$painted_windows"

if [ "$app_exited_before_deadline" -eq 1 ]; then
  printf 'TOR-4902-ERROR app exited before second 3\n' >&2
  exit 3
fi

if [ "$DISABLE_TOR_STARTER" = "1" ]; then
  if [ "$painted_windows" -eq 1 ]; then
    printf 'TOR-4902-INSTRUMENT\n'
    exit 0
  fi
  printf 'TOR-4902-INSTRUMENT-MISS\n' >&2
  exit 1
fi

if [ "$painted_windows" -eq 0 ]; then
  printf 'TOR-4902-RED\n'
  exit 1
fi

printf 'TOR-4902-UNEXPECTED-GREEN\n' >&2
exit 2
