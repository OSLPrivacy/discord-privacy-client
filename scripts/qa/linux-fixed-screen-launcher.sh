#!/usr/bin/env bash
set -euo pipefail

readonly OSL_FIXED_SCREEN_SIZE="1280x800"
readonly OSL_FIXED_SCREEN_DEPTH="24"
readonly OSL_FIXED_SCREEN_SCALE="1"
readonly OSL_FIXED_SCREEN_THEME="dark"
readonly OSL_FIXED_SCREEN_DPI="96"

display="${OSL_FIXED_SCREEN_DISPLAY:-:90}"

usage() {
  printf 'usage: %s [--display :N] [-- command ...]\n' "$0" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --display)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      display="$2"
      shift 2
      ;;
    --help|-h)
set -Eeuo pipefail

readonly OSL_FIXED_SCREEN_SIZE="1280x800"
readonly OSL_FIXED_SCREEN_DEPTH="24"
readonly OSL_FIXED_SCREEN_GEOMETRY="${OSL_FIXED_SCREEN_SIZE}x${OSL_FIXED_SCREEN_DEPTH}"
readonly OSL_FIXED_SCREEN_DPI="96"
readonly OSL_FIXED_SCREEN_SCALE="1"
readonly OSL_FIXED_SCREEN_THEME="dark"

display=""
xvfb_pid=""
xvfb_log=""
child_pid=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/qa/linux-fixed-screen-launcher.sh [--display :N] [-- <command> [args...]]

Starts the OSL Linux QA fake screen with the fixed launch contract:
  size=1280x800 color_depth=24 scale=1 theme=dark

With a command, runs it inside the fixed display and monitors the display until
the command exits. Without a command, starts the display, verifies the launch
contract, prints it, and exits.
USAGE
}

die() {
  printf 'linux-fixed-screen-launcher: %s\n\n' "$1" >&2
  usage >&2
  exit 2
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'linux-fixed-screen-launcher: missing command: %s\n' "$1" >&2
    printf 'linux-fixed-screen-launcher: install packages: xvfb x11-utils\n' >&2
    exit 69
  }
}

choose_display() {
  local n
  for n in $(seq 90 199); do
    if [[ ! -e "/tmp/.X11-unix/X${n}" && ! -e "/tmp/.X${n}-lock" ]]; then
      printf ':%s\n' "$n"
      return 0
    fi
  done
  return 1
}

display_alive() {
  xdpyinfo -display "$display" >/dev/null 2>&1
}

process_running() {
  local pid="$1"
  local state
  state="$(ps -o stat= -p "$pid" 2>/dev/null || true)"
  [[ -n "$state" && "$state" != Z* ]]
}

stop_child() {
  if [[ -n "$child_pid" ]] && process_running "$child_pid"; then
    kill "$child_pid" >/dev/null 2>&1 || true
    wait "$child_pid" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  local rc=$?
  stop_child
  if [[ -n "$xvfb_pid" ]] && kill -0 "$xvfb_pid" >/dev/null 2>&1; then
    kill "$xvfb_pid" >/dev/null 2>&1 || true
    wait "$xvfb_pid" >/dev/null 2>&1 || true
  fi
  [[ -n "$xvfb_log" ]] && rm -f "$xvfb_log"
  exit "$rc"
}

wait_for_ready_display() {
  local tries=0
  while [[ "$tries" -lt 80 ]]; do
    if display_alive; then
      return 0
    fi
    if ! kill -0 "$xvfb_pid" >/dev/null 2>&1; then
      printf 'linux-fixed-screen-launcher: Xvfb exited before %s was ready\n' "$display" >&2
      sed -n '1,40p' "$xvfb_log" >&2 || true
      return 1
    fi
    tries=$((tries + 1))
    sleep 0.1
  done
  printf 'linux-fixed-screen-launcher: timed out waiting for %s\n' "$display" >&2
  sed -n '1,40p' "$xvfb_log" >&2 || true
  return 1
}

xdpyinfo_value() {
  local pattern="$1"
  xdpyinfo -display "$display" | sed -n "$pattern" | sed -n '1p'
}

verify_fixed_contract() {
  local actual_size actual_depth actual_dpi
  actual_size="$(xdpyinfo_value 's/^  dimensions:[[:space:]]*\([0-9]\+x[0-9]\+\) pixels.*/\1/p')"
  actual_depth="$(xdpyinfo_value 's/^  depth of root window:[[:space:]]*\([0-9]\+\) planes.*/\1/p')"
  actual_dpi="$(xdpyinfo_value 's/^  resolution:[[:space:]]*\([0-9]\+x[0-9]\+\) dots per inch.*/\1/p')"

  if [[ "$actual_size" != "$OSL_FIXED_SCREEN_SIZE" ]]; then
    printf 'linux-fixed-screen-launcher: size check failed: expected %s got %s\n' \
      "$OSL_FIXED_SCREEN_SIZE" "${actual_size:-unknown}" >&2
    return 70
  fi
  if [[ "$actual_depth" != "$OSL_FIXED_SCREEN_DEPTH" ]]; then
    printf 'linux-fixed-screen-launcher: color_depth check failed: expected %s got %s\n' \
      "$OSL_FIXED_SCREEN_DEPTH" "${actual_depth:-unknown}" >&2
    return 71
  fi
  if [[ "$actual_dpi" != "${OSL_FIXED_SCREEN_DPI}x${OSL_FIXED_SCREEN_DPI}" ]]; then
    printf 'linux-fixed-screen-launcher: dpi check failed: expected %sx%s got %s\n' \
      "$OSL_FIXED_SCREEN_DPI" "$OSL_FIXED_SCREEN_DPI" "${actual_dpi:-unknown}" >&2
    return 72
  fi

  printf 'OSL_FIXED_SCREEN display=%s size=%s color_depth=%s scale=%s theme=%s dpi=%sx%s\n' \
    "$DISPLAY" \
    "$actual_size" \
    "$actual_depth" \
    "$OSL_FIXED_SCREEN_SCALE" \
    "$OSL_FIXED_SCREEN_THEME" \
    "$OSL_FIXED_SCREEN_DPI" \
    "$OSL_FIXED_SCREEN_DPI"
}

apply_fixed_environment() {
  export DISPLAY="$display"
  export GDK_SCALE="$OSL_FIXED_SCREEN_SCALE"
  export GDK_DPI_SCALE="$OSL_FIXED_SCREEN_SCALE"
  export QT_SCALE_FACTOR="$OSL_FIXED_SCREEN_SCALE"
  export QT_AUTO_SCREEN_SCALE_FACTOR=0
  export QT_ENABLE_HIGHDPI_SCALING=0
  export WINIT_X11_SCALE_FACTOR="$OSL_FIXED_SCREEN_SCALE"
  export GTK_THEME="Adwaita:${OSL_FIXED_SCREEN_THEME}"
  export OSL_QA_FIXED_SCREEN_THEME="$OSL_FIXED_SCREEN_THEME"
  export OSL_QA_FIXED_SCREEN_SCALE="$OSL_FIXED_SCREEN_SCALE"
  export OSL_QA_FIXED_SCREEN_SIZE="$OSL_FIXED_SCREEN_SIZE"

  printf 'Xft.dpi: %s\nNet/ThemeName: Adwaita-dark\n' "$OSL_FIXED_SCREEN_DPI" |
    xrdb -display "$DISPLAY" -merge
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --display)
      [[ "$#" -ge 2 ]] || die "missing value for --display"
      display="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      usage
      exit 64
      die "unknown option: $1"
      ;;
    *)
      break
      ;;
  esac
done

xvfb_pid=""
cleanup() {
  if [ -n "$xvfb_pid" ] && kill -0 "$xvfb_pid" 2>/dev/null; then
    kill "$xvfb_pid" 2>/dev/null || true
    wait "$xvfb_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

Xvfb "$display" \
  -screen 0 "${OSL_FIXED_SCREEN_SIZE}x${OSL_FIXED_SCREEN_DEPTH}" \
  -dpi "$OSL_FIXED_SCREEN_DPI" \
  -nolisten tcp \
  >/dev/null 2>&1 &
xvfb_pid="$!"

for _ in $(seq 1 50); do
  if DISPLAY="$display" xdpyinfo >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$xvfb_pid" 2>/dev/null; then
    printf 'linux-fixed-screen-launcher: Xvfb exited before readiness: %s\n' "$display" >&2
    exit 69
  fi
  sleep 0.1
done

info="$(DISPLAY="$display" xdpyinfo 2>/dev/null)" || {
  printf 'linux-fixed-screen-launcher: fake screen did not become ready: %s\n' "$display" >&2
  exit 69
}

actual_size="$(printf '%s\n' "$info" | awk '/dimensions:/{print $2; exit}')"
if [ "$actual_size" != "$OSL_FIXED_SCREEN_SIZE" ]; then
  printf 'linux-fixed-screen-launcher: size check failed: expected %s got %s\n' \
    "$OSL_FIXED_SCREEN_SIZE" "${actual_size:-missing}" >&2
  exit 70
fi

actual_depth="$(printf '%s\n' "$info" | awk '/depth of root window:/{print $5; exit}')"
if [ "$actual_depth" != "$OSL_FIXED_SCREEN_DEPTH" ]; then
  printf 'linux-fixed-screen-launcher: colour mode check failed: expected %s got %s\n' \
    "$OSL_FIXED_SCREEN_DEPTH" "${actual_depth:-missing}" >&2
  exit 71
fi

dpi_pair="$(printf '%s\n' "$info" | awk '/resolution:/{print $2; exit}')"
if [ "$dpi_pair" != "${OSL_FIXED_SCREEN_DPI}x${OSL_FIXED_SCREEN_DPI}" ]; then
  printf 'linux-fixed-screen-launcher: dpi check failed: expected %sx%s got %s\n' \
    "$OSL_FIXED_SCREEN_DPI" "$OSL_FIXED_SCREEN_DPI" "${dpi_pair:-missing}" >&2
  exit 72
fi

export DISPLAY="$display"
export OSL_FIXED_SCREEN_SIZE
export OSL_FIXED_SCREEN_DEPTH
export OSL_FIXED_SCREEN_SCALE
export OSL_FIXED_SCREEN_THEME
export OSL_FIXED_SCREEN_DPI
export GDK_SCALE="$OSL_FIXED_SCREEN_SCALE"
export GDK_DPI_SCALE="1"
export QT_SCALE_FACTOR="$OSL_FIXED_SCREEN_SCALE"
export QT_AUTO_SCREEN_SCALE_FACTOR="0"
export QT_ENABLE_HIGHDPI_SCALING="0"
export GTK_THEME="Adwaita-dark"
export OSL_FAKE_SCREEN_THEME="$OSL_FIXED_SCREEN_THEME"

printf 'OSL_FIXED_SCREEN display=%s size=%s color_depth=%s scale=%s theme=%s dpi=%sx%s\n' \
  "$display" \
  "$OSL_FIXED_SCREEN_SIZE" \
  "$OSL_FIXED_SCREEN_DEPTH" \
  "$OSL_FIXED_SCREEN_SCALE" \
  "$OSL_FIXED_SCREEN_THEME" \
  "$OSL_FIXED_SCREEN_DPI" \
  "$OSL_FIXED_SCREEN_DPI"

if [ "$#" -gt 0 ]; then
  "$@"
fi
require_command Xvfb
require_command xdpyinfo
require_command xrdb

if [[ -z "$display" ]]; then
  display="$(choose_display)" || die "no free X display found in :90..:199"
fi
[[ "$display" == :* ]] || die "--display must look like :99"

xvfb_log="$(mktemp)"
trap cleanup EXIT INT TERM
Xvfb "$display" -screen 0 "$OSL_FIXED_SCREEN_GEOMETRY" -dpi "$OSL_FIXED_SCREEN_DPI" -nolisten tcp >"$xvfb_log" 2>&1 &
xvfb_pid=$!

wait_for_ready_display
apply_fixed_environment
verify_fixed_contract

if [[ "$#" -eq 0 ]]; then
  exit 0
fi

"$@" &
child_pid=$!

while process_running "$child_pid"; do
  if ! display_alive; then
    printf 'linux-fixed-screen-launcher: fake screen stopped while command was running: %s\n' "$DISPLAY" >&2
    stop_child
    exit 97
  fi
  sleep 0.2
done

set +e
wait "$child_pid"
child_rc=$?
set -e
exit "$child_rc"
