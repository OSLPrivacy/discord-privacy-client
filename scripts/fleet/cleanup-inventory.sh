#!/usr/bin/env bash
# Read-only preflight for scheduled cleanup.  It never removes, modifies, or
# stages repository data; callers may redirect its timestamped report to storage.
set -euo pipefail

repo="$(pwd)"
lock_paths=()
crash_markers=()
while (($#)); do
  case "$1" in
    --repo) repo="$2"; shift 2 ;;
    --lock-path) lock_paths+=("$2"); shift 2 ;;
    --crash-marker) crash_markers+=("$2"); shift 2 ;;
    *) echo "usage: $0 [--repo PATH] [--lock-path PATH] [--crash-marker PATH]" >&2; exit 64 ;;
  esac
done
repo="$(cd "$repo" && pwd -P)"
if ! git -C "$repo" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "cleanup_blocked: repo is not a git worktree: $repo" >&2
  exit 3
fi
if ((${#lock_paths[@]} == 0)); then
  lock_paths=("/tmp/osl-cargo.lock" "$repo/target/.cargo-lock")
fi
if ((${#crash_markers[@]} == 0)); then
  crash_markers=("$repo/.osl-crash-marker" "$repo/.crash-marker" "$repo/crash.marker")
fi

blocked=()
emit() { printf '%s: %s\n' "$1" "$2"; }
fingerprint() { sha256sum "$1" | awk '{print $1}'; }

emit format cleanup-inventory-v1
emit timestamp_utc "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
emit repository "$repo"
emit disk_use_bytes "$(du -sb "$repo" | awk '{print $1}')"
emit worktrees "$(git -C "$repo" worktree list --porcelain | awk '/^worktree /{printf "%s;", substr($0,10)}')"
emit branches "$(git -C "$repo" for-each-ref --format='%(refname:short)' refs/heads | tr '\n' ';')"
emit heads "$(git -C "$repo" show-ref --heads | awk '{printf "%s;", $1}')"
emit upstreams "$(git -C "$repo" for-each-ref --format='%(refname:short)=%(upstream:short)' refs/heads | tr '\n' ';')"

dirty="$(git -C "$repo" status --porcelain)"
dirty_fingerprints=""
untracked_fingerprints=""
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  path="${line:3}"
  if [[ "$line" == \?\?* ]]; then
    if [[ -f "$repo/$path" ]]; then untracked_fingerprints+="$path=$(fingerprint "$repo/$path");"; else untracked_fingerprints+="$path=non-file;"; fi
  else
    if [[ -f "$repo/$path" ]]; then dirty_fingerprints+="$path=$(fingerprint "$repo/$path");"; else dirty_fingerprints+="$path=non-file;"; fi
  fi
done <<< "$dirty"
emit dirty_fingerprints "${dirty_fingerprints:-none}"
emit untracked_fingerprints "${untracked_fingerprints:-none}"
[[ -n "$dirty" ]] && blocked+=(dirty-worktree)

stash_count="$(git -C "$repo" stash list | wc -l | tr -d ' ')"
emit stashes "$stash_count"
[[ "$stash_count" != 0 ]] && blocked+=(stash-present)

lock_report=""
for lock in "${lock_paths[@]}"; do
  if [[ -e "$lock" ]]; then
    owner="$(stat -c '%U' "$lock" 2>/dev/null || echo unknown)"
    if flock -n "$lock" -c true 2>/dev/null; then state=present-unlocked; else state=active; blocked+=(active-lock:"$lock"); fi
    pids="$(fuser "$lock" 2>/dev/null | tr '\n' ',' || true)"
    lock_report+="$lock:$state:owner=$owner:pids=${pids:-none};"
  else
    lock_report+="$lock:absent;"
  fi
done
emit active_processes "${lock_report:-none}"
emit locks "${lock_report:-none}"

marker_report=""
for marker in "${crash_markers[@]}"; do
  if [[ -e "$marker" ]]; then marker_report+="$marker;"; blocked+=(crash-uncertainty:"$marker"); fi
done
emit crash_markers "${marker_report:-none}"
artifact_summary="$(find "$repo" -path '*/target' -o -path '*/dist' -o -path '*/node_modules' 2>/dev/null | head -20 | while read -r p; do stat -c '%Y:%n' "$p"; done | tr '\n' ';')"
emit artifact_age_reference "${artifact_summary:-none}"
emit recovery_plan "retain this report; resolve every blocker, obtain target approval, then run reclaim.sh (never rm -rf)"
emit expected_reclaimed_bytes "not estimated until an approved reclaim target is compared against this inventory"
if ((${#blocked[@]})); then
  emit result cleanup_blocked
  emit blockers "$(IFS=,; echo "${blocked[*]}")"
  exit 3
fi
emit result cleanup_ready
emit blockers none
