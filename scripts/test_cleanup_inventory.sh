#!/usr/bin/env bash
# T8-T09a: prove cleanup inventory blocks without mutating a seeded repository.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
inventory="$root/scripts/fleet/cleanup-inventory.sh"
tmp="$(mktemp -d)"
trap 'kill "${holder:-}" 2>/dev/null || true; rm -rf "$tmp"' EXIT
repo="$tmp/repo"
git init -q "$repo"
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" config user.name fixture
printf 'base\n' > "$repo/tracked.txt"
git -C "$repo" add tracked.txt && git -C "$repo" commit -qm base

clean="$tmp/clean.txt"
"$inventory" --repo "$repo" --lock-path "$tmp/clean.lock" --crash-marker "$tmp/no-marker" > "$clean"
for field in format timestamp_utc disk_use_bytes worktrees branches heads upstreams dirty_fingerprints untracked_fingerprints stashes active_processes locks artifact_age_reference recovery_plan expected_reclaimed_bytes result; do
  grep -q "^$field:" "$clean"
done
grep -q '^result: cleanup_ready$' "$clean"

printf 'dirty\n' >> "$repo/tracked.txt"
printf 'stash\n' > "$repo/stash.txt"
git -C "$repo" add stash.txt && git -C "$repo" stash push -qm seeded-stash
printf 'dirty-again\n' >> "$repo/tracked.txt"
lock="$tmp/build.lock"; : > "$lock"; flock "$lock" sleep 30 & holder=$!
marker="$tmp/crash.marker"; : > "$marker"
before="$(find "$repo" -path "$repo/.git" -prune -o -printf '%P:%s:%T@\n' | sort)"
blocked="$tmp/blocked.txt"
if "$inventory" --repo "$repo" --lock-path "$lock" --crash-marker "$marker" > "$blocked"; then
  echo 'seeded inventory unexpectedly became cleanup_ready' >&2; exit 1
fi
after="$(find "$repo" -path "$repo/.git" -prune -o -printf '%P:%s:%T@\n' | sort)"
[[ "$before" == "$after" ]]
grep -q '^result: cleanup_blocked$' "$blocked"
grep -q 'dirty-worktree' "$blocked"
grep -q 'stash-present' "$blocked"
grep -q 'active-lock:' "$blocked"
grep -q 'crash-uncertainty:' "$blocked"
