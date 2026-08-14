#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
note="$root/docs/release/release-scope-exclusions.md"
backup="$(mktemp)"
cp "$note" "$backup"
restore() {
  cp "$backup" "$note"
  rm -f "$backup"
}
trap restore EXIT

release_notes() {
  python3 "$root/scripts/extract_changelog_section.py" \
    --tag hub-v0.1.0 \
    --changelog "$root/CHANGELOG.md" \
    --scope-note "$note"
}

tile_test() {
  (cd "$root/apps/osl-hub-ui" && ./node_modules/.bin/vitest run src/task-3528-release-scope-note.test.ts --maxWorkers=1 --minWorkers=1)
}

baseline_notes="$(release_notes)"
grep -q '^### Deliberately not in this release$' <<<"$baseline_notes"
grep -q '\*\*Code signing\*\* (2026-07-31).*ship unsigned' <<<"$baseline_notes"
grep -q '\*\*OSL Notes\*\* (2026-08-06).*OSL Notes was already out\.' <<<"$baseline_notes"
grep -q '\*\*OSL Mail\*\* (2026-08-06).*osl mail is on hold until the other stuff\.' <<<"$baseline_notes"
tile_test
printf 'TASK3528_BASELINE_RELEASE_ITEMS=3\n'

sed -i \
  -e 's/"tile_label": "Not started"/"tile_label": "Held by owner"/g' \
  -e 's/OSL Mail is not in this release and is not being built for it\./OSL Mail MUTATED release boundary./' \
  "$note"

mutated_notes="$(release_notes)"
grep -q 'OSL Mail MUTATED release boundary\.' <<<"$mutated_notes"
TASK3528_EXPECT_MAIL_LABEL='Held by owner' \
TASK3528_EXPECT_NOTES_LABEL='Held by owner' \
  tile_test
printf 'TASK3528_MUTATED_RELEASE_NOTE=OSL Mail MUTATED release boundary.\n'
printf 'TASK3528_MUTATED_TILE_LABELS=OSL Mail:Held by owner|OSL Notes:Held by owner\n'

restore
trap - EXIT
restored_notes="$(release_notes)"
grep -q 'OSL Mail is not in this release and is not being built for it\.' <<<"$restored_notes"
if grep -q 'MUTATED' <<<"$restored_notes"; then
  printf 'TASK3528_ERROR=mutation survived restoration\n' >&2
  exit 1
fi
tile_test
printf 'TASK3528_RESTORED=1\n'
