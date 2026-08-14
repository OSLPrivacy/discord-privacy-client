#!/usr/bin/env bash
set -euo pipefail

audit="$(dirname "$0")/check-task-3527-osl-dependencies.py"
task_root="/home/liamw/osl-plan/OSL-AUDITS/todo"

green_output="$(python3 "$audit" --task-root "$task_root")"
printf '%s\n' "$green_output"
grep -qx 'TASK3527_MENTIONING_TASKS=20' <<<"$green_output"
grep -qx 'TASK3527_WITH_NEITHER_LABEL_NOR_BLOCKER=0' <<<"$green_output"
grep -qx 'TASK3527_NEED_OSL_MAIL_OR_NOTES_TO_WORK=0' <<<"$green_output"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cp -a "$task_root/." "$scratch/"
printf '\nTASK 9999 - injected live OSL Mail dependency\ndo: This task needs OSL Mail to exist and work before its check can pass.\ndone when: it works.\n' >>"$scratch/12-final-audit.txt"

set +e
red_output="$(python3 "$audit" --task-root "$scratch" 2>&1)"
red_status=$?
set -e
printf '%s\n' "$red_output"
printf 'TASK3527B_RED_EXIT=%s\n' "$red_status"
test "$red_status" -eq 1
grep -q 'task=9999 blocker=live-osl-dependency' <<<"$red_output"
grep -qx 'TASK3527_NEED_OSL_MAIL_OR_NOTES_TO_WORK=1' <<<"$red_output"

# The authoritative corpus was never changed; show its green result again
# after the red fixture has been discarded.
restored_output="$(python3 "$audit" --task-root "$task_root")"
grep -qx 'TASK3527_MENTIONING_TASKS=20' <<<"$restored_output"
grep -qx 'TASK3527_WITH_NEITHER_LABEL_NOR_BLOCKER=0' <<<"$restored_output"
grep -qx 'TASK3527_NEED_OSL_MAIL_OR_NOTES_TO_WORK=0' <<<"$restored_output"
printf 'TASK3527B_RESTORED_GREEN=1\n'
