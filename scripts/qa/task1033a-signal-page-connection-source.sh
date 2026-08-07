#!/usr/bin/env bash
set -euo pipefail

root="${OSL_PLAN_ROOT:-/home/liamw/osl-plan/OSL-AUDITS}"
source_note="$root/evidence/3402.md"
this_note="$root/evidence/1033a.md"

if [[ ! -f "$source_note" ]]; then
  echo "TASK1033A_ERROR=missing 3402 evidence at $source_note" >&2
  exit 1
fi

signal_pattern='^- Signal: (answered|did not answer); version [^;]+; exact command used: `.*--remote-debugging-port=.*--user-data-dir=.*`'
signal_line="$(grep -aE "$signal_pattern" "$source_note" || true)"
signal_line_count="$( (grep -aE "$signal_pattern" "$source_note" || true) | wc -l | tr -d '[:space:]')"

second_note_count="$(
  (
    find "$root/evidence" -maxdepth 1 -type f -name '*.md' \
      ! -name '3402.md' \
      ! -name '1033a.md' \
      -print0 \
      | xargs -0 -r grep -aEl 'Signal:.*--remote-debugging-port=|Signal.*page[- ]connection|page[- ]connection.*Signal' \
  ) || true
)"
second_note_count="$(
  printf '%s\n' "$second_note_count" \
    | sed '/^$/d' \
    | wc -l \
    | tr -d '[:space:]'
)"

echo "TASK1033A_SOURCE_TASK=3402"
echo "TASK1033A_SOURCE_EVIDENCE=$source_note"
echo "TASK1033A_SIGNAL_PAGE_CONNECTION_RESULT_COUNT=$signal_line_count"
echo "TASK1033A_SIGNAL_LINE=$signal_line"
echo "TASK1033A_SECOND_SIGNAL_PROBE_OR_NOTE_COUNT=$second_note_count"

if [[ "$signal_line_count" != "1" ]]; then
  echo "TASK1033A_ERROR=expected exactly one Signal page-connection result in task 3402 evidence" >&2
  exit 1
fi

if [[ "$second_note_count" != "0" ]]; then
  echo "TASK1033A_ERROR=found a second Signal page-connection probe or note outside 3402" >&2
  exit 1
fi

if [[ -f "$this_note" ]]; then
  echo "TASK1033A_EVIDENCE_NOTE=$this_note"
else
  echo "TASK1033A_EVIDENCE_NOTE=not-yet-written"
fi
