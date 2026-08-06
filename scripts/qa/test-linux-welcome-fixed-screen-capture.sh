#!/usr/bin/env bash
set -euo pipefail

repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
checker="$repo/scripts/qa/check-linux-welcome-capture.sh"
artifact_dir="$repo/evidence/task-0027"
image="$artifact_dir/linux-welcome-fixed-screen.png"
switches="$artifact_dir/linux-welcome-fixed-screen.switches.txt"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

green="$tmp_dir/green.out"
red_out="$tmp_dir/red.out"
red_err="$tmp_dir/red.err"
blank="$tmp_dir/linux-welcome-fixed-screen.blank.png"

"$checker" --image "$image" --switches "$switches" >"$green"

convert -size 1280x800 xc:black "$blank"

set +e
"$checker" --image "$blank" --switches "$switches" >"$red_out" 2>"$red_err"
blank_rc="$?"
set -e

if [ "$blank_rc" -eq 0 ]; then
  printf 'TASK0027_BLANK_REPLACEMENT_UNEXPECTED_PASS image=%s\n' "$blank" >&2
  exit 1
fi

if ! grep -Fq 'TASK0027_CAPTURE_CHECK_PASS' "$green"; then
  printf 'TASK0027_REAL_CAPTURE_DID_NOT_PASS\n' >&2
  exit 1
fi

if ! grep -Fq 'TASK0027_CAPTURE_CHECK_FAIL reason=blank-or-low-detail distinct_colors=1 minimum=64' "$red_err"; then
  printf 'TASK0027_BLANK_REPLACEMENT_DID_NOT_FAIL_AS_BLANK\n' >&2
  cat "$red_err" >&2
  exit 1
fi

cat "$green"
cat "$red_out"
cat "$red_err"
printf 'TASK0027_BLANK_REPLACEMENT_CHECK rc=%s result=fail\n' "$blank_rc"
