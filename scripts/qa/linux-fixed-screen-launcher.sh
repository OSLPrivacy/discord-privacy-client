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
