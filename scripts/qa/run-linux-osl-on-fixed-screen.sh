#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
launcher="${OSL_LINUX_FIXED_SCREEN_LAUNCHER:-$script_dir/linux-fixed-screen-launcher.sh}"
record=""
display_args=()
switches=()

usage() {
  printf 'usage: %s --record PATH [--display :N] [--switch name=value ...] -- command ...\n' "$0" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --record)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      record="$2"
      shift 2
      ;;
    --display)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      display_args=(--display "$2")
      shift 2
      ;;
    --switch)
      if [ "$#" -lt 2 ]; then
        usage
        exit 64
      fi
      switches+=("$2")
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

if [ -z "$record" ] || [ "$#" -eq 0 ]; then
  usage
  exit 64
fi

if [ "${#switches[@]}" -gt 0 ]; then
  switch_string="${switches[*]}"
else
  switch_string="${OSL_TEST_ONLY_RUNTIME_SWITCHES:-}"
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

record_tmp="$tmp_dir/record"
launcher_stdout="$tmp_dir/launcher.out"
launcher_stderr="$tmp_dir/launcher.err"
child_stdout="$tmp_dir/child.out"
child_stderr="$tmp_dir/child.err"

set +e
OSL_TEST_ONLY_RUNTIME_SWITCHES="$switch_string" "$launcher" "${display_args[@]}" -- \
  bash -c '
    set -euo pipefail
    record_tmp="$1"
    child_stdout="$2"
    child_stderr="$3"
    shift 3

    if [ -z "${DISPLAY:-}" ]; then
      printf "linux-osl-fixed-screen-run: missing DISPLAY\n" >&2
      exit 1
    fi

    "$@" >"$child_stdout" 2>"$child_stderr" &
    pid="$!"
    {
      printf "process_number=%s\n" "$pid"
      printf "switches=%s\n" "${OSL_TEST_ONLY_RUNTIME_SWITCHES:-}"
      printf "display=%s\n" "$DISPLAY"
      printf "size=%s\n" "${OSL_FIXED_SCREEN_SIZE:-missing}"
      printf "color_depth=%s\n" "${OSL_FIXED_SCREEN_DEPTH:-missing}"
      printf "scale=%s\n" "${OSL_FIXED_SCREEN_SCALE:-missing}"
      printf "theme=%s\n" "${OSL_FIXED_SCREEN_THEME:-missing}"
      printf "dpi=%sx%s\n" "${OSL_FIXED_SCREEN_DPI:-missing}" "${OSL_FIXED_SCREEN_DPI:-missing}"
      printf "gdk_scale=%s\n" "${GDK_SCALE:-missing}"
      printf "qt_scale_factor=%s\n" "${QT_SCALE_FACTOR:-missing}"
      printf "gtk_theme=%s\n" "${GTK_THEME:-missing}"
    } >"$record_tmp"
    wait "$pid"
  ' bash "$record_tmp" "$child_stdout" "$child_stderr" "$@" \
  >"$launcher_stdout" 2>"$launcher_stderr"
run_rc="$?"
set -e

cat "$launcher_stdout"
[ -f "$child_stdout" ] && cat "$child_stdout"
cat "$launcher_stderr" >&2
[ -f "$child_stderr" ] && cat "$child_stderr" >&2

if [ "$run_rc" -ne 0 ]; then
  exit 1
fi

if [ ! -s "$record_tmp" ]; then
  printf 'linux-osl-fixed-screen-run: process record was not written\n' >&2
  exit 1
fi

install -m 0644 "$record_tmp" "$record"
process_number="$(sed -n 's/^process_number=//p' "$record" | head -n 1)"
if ! printf '%s\n' "$process_number" | grep -Eq '^[0-9]+$'; then
  printf 'linux-osl-fixed-screen-run: process number is missing or invalid\n' >&2
  exit 1
fi

printf 'OSL_LINUX_FAKE_SCREEN_RUN opened=true process_number=%s switches=%s record=%s\n' \
  "$process_number" \
  "$switch_string" \
  "$record"
