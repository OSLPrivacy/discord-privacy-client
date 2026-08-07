#!/usr/bin/env bash
set -Eeuo pipefail

readonly REQUIRED_PACKAGES="xvfb x11-utils xdotool imagemagick"
readonly OPTIONAL_SMOKE_PACKAGE="x11-apps"
display=""
geometry="${OSL_FAKE_SCREEN_GEOMETRY:-1280x800x24}"
capture_window=""
output_path=""
window_timeout="${OSL_FAKE_SCREEN_WINDOW_TIMEOUT:-20}"
print_packages=0
xvfb_pid=""
child_pid=""
xvfb_log=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/qa/linux-fake-screen.sh [--display :N] [--geometry WxHxD]
  scripts/qa/linux-fake-screen.sh [options] -- <command> [args...]
  scripts/qa/linux-fake-screen.sh --capture-window <title-regex> --output <png> -- <command> [args...]

Starts a private Xvfb display, prints DISPLAY=<name>, and runs the optional
command inside it. With --capture-window it waits for one visible matching
window and captures that window with ImageMagick import.

Debian/Ubuntu packages:
  sudo apt-get update && sudo apt-get install -y xvfb x11-utils xdotool imagemagick

Optional smoke-test package:
  sudo apt-get install -y x11-apps

Examples:
  scripts/qa/linux-fake-screen.sh
  scripts/qa/linux-fake-screen.sh -- xeyes
  CARGO_TARGET_DIR=/home/liamw/osl-exec-e/.cargo-target \
    scripts/qa/linux-fake-screen.sh --capture-window 'OSL Privacy' \
      --output /tmp/osl-window.png -- cargo tauri dev \
      --manifest-path apps/osl-hub/Cargo.toml --features desktop
USAGE
}

die() {
  printf 'linux-fake-screen: %s\n\n' "$1" >&2
  usage >&2
  exit 2
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'linux-fake-screen: missing command: %s\n' "$1" >&2
    printf 'linux-fake-screen: install packages: %s\n' "$REQUIRED_PACKAGES" >&2
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
      printf 'linux-fake-screen: Xvfb exited before %s was ready\n' "$display" >&2
      sed -n '1,40p' "$xvfb_log" >&2 || true
      return 1
    fi
    tries=$((tries + 1))
    sleep 0.1
  done
  printf 'linux-fake-screen: timed out waiting for %s\n' "$display" >&2
  sed -n '1,40p' "$xvfb_log" >&2 || true
  return 1
}

first_matching_window() {
  local ids
  ids="$(xdotool search --onlyvisible --name "$capture_window" 2>/dev/null || true)"
  printf '%s\n' "$ids" | sed -n '1p'
}

capture_one_window() {
  local deadline=$((SECONDS + window_timeout))
  local window_id=""

  while [[ "$SECONDS" -lt "$deadline" ]]; do
    if ! display_alive; then
      printf 'linux-fake-screen: fake screen stopped before capture: %s\n' "$display" >&2
      return 97
    fi
    if ! process_running "$child_pid"; then
      printf 'linux-fake-screen: child command exited before a matching window appeared\n' >&2
      return 98
    fi
    window_id="$(first_matching_window)"
    if [[ -n "$window_id" ]]; then
      mkdir -p -- "$(dirname -- "$output_path")"
      import -window "$window_id" "$output_path"
      printf 'WINDOW_ID=%s\n' "$window_id"
      printf 'CAPTURE=%s\n' "$output_path"
      return 0
    fi
    sleep 0.2
  done

  printf 'linux-fake-screen: timed out waiting for window matching %q\n' "$capture_window" >&2
  return 99
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --packages)
      print_packages=1
      shift
      ;;
    --display)
      [[ "$#" -ge 2 ]] || die "missing value for --display"
      display="$2"
      shift 2
      ;;
    --geometry)
      [[ "$#" -ge 2 ]] || die "missing value for --geometry"
      geometry="$2"
      shift 2
      ;;
    --capture-window)
      [[ "$#" -ge 2 ]] || die "missing value for --capture-window"
      capture_window="$2"
      shift 2
      ;;
    --output)
      [[ "$#" -ge 2 ]] || die "missing value for --output"
      output_path="$2"
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
      die "unknown option: $1"
      ;;
    *)
      break
      ;;
  esac
done

if [[ "$print_packages" -eq 1 ]]; then
  printf 'Debian/Ubuntu packages: %s\n' "$REQUIRED_PACKAGES"
  printf 'Optional smoke-test package: %s\n' "$OPTIONAL_SMOKE_PACKAGE"
  printf 'Install: sudo apt-get update && sudo apt-get install -y %s\n' "$REQUIRED_PACKAGES"
  exit 0
fi

if [[ -n "$capture_window" && -z "$output_path" ]]; then
  die "--capture-window requires --output"
fi
if [[ -z "$capture_window" && -n "$output_path" ]]; then
  die "--output requires --capture-window"
fi
if [[ -n "$capture_window" && "$#" -eq 0 ]]; then
  die "--capture-window requires a command to open the target window"
fi

require_command Xvfb
require_command xdpyinfo
if [[ -n "$capture_window" ]]; then
  require_command xdotool
  require_command import
fi

if [[ -z "$display" ]]; then
  display="$(choose_display)" || die "no free X display found in :90..:199"
fi
[[ "$display" == :* ]] || die "--display must look like :99"

xvfb_log="$(mktemp)"
trap cleanup EXIT INT TERM
Xvfb "$display" -screen 0 "$geometry" -nolisten tcp >"$xvfb_log" 2>&1 &
xvfb_pid=$!

wait_for_ready_display
export DISPLAY="$display"
printf 'DISPLAY=%s\n' "$DISPLAY"
printf 'XVFB_PID=%s\n' "$xvfb_pid"

if [[ "$#" -eq 0 ]]; then
  while kill -0 "$xvfb_pid" >/dev/null 2>&1 && display_alive; do
    sleep 0.2
  done
  printf 'linux-fake-screen: fake screen stopped: %s\n' "$DISPLAY" >&2
  exit 97
fi

"$@" &
child_pid=$!

if [[ -n "$capture_window" ]]; then
  set +e
  capture_one_window
  capture_rc=$?
  set -e
  stop_child
  exit "$capture_rc"
fi

while process_running "$child_pid"; do
  if ! display_alive; then
    printf 'linux-fake-screen: fake screen stopped while command was running: %s\n' "$DISPLAY" >&2
    stop_child
    exit 97
  fi
  sleep 0.2
done

wait "$child_pid"
