#!/usr/bin/env bash
# TASK 7001 acceptance checker. Every named stage is mandatory; TASK7001_SKIP
# exists solely to prove that a starved proof fails closed.
set -euo pipefail

repo=$(git rev-parse --show-toplevel)
cd "$repo"
skip=",${TASK7001_SKIP:-},"

require_stage() {
  local stage=$1
  if [[ $skip == *",$stage,"* ]]; then
    echo "TASK7001_FAIL absent_starvation=$stage" >&2
    exit 1
  fi
}

for stage in cargo-check public-comparison map-replay two-lane-measurement; do
  require_stage "$stage"
done

run_cargo_check() {
  CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i \
    cargo check --manifest-path apps/osl-hub/Cargo.toml --no-default-features --features core --lib
  echo "TASK7001_CARGO_CHECK exit=0 errors=0"
}

compare_public_items() {
  local actual
  actual=$(mktemp)
  trap 'rm -f "$actual"' RETURN
  awk '/^pub mod [A-Za-z_][A-Za-z0-9_]*[ ;{]/{name=$3; sub(/[;{]/,"",name); print name}' \
    apps/osl-hub/src/module_declarations_*.rs | sort -u > "$actual"
  diff -u tools/task-7001-check/public-modules.before "$actual"
  echo "TASK7001_PUBLIC_COMPARISON before=$(wc -l < tools/task-7001-check/public-modules.before) after=$(wc -l < "$actual") identical=1"
}

replay_map() {
  local commit module destination parent count=0
  while IFS=$'\t' read -r commit module destination; do
    [[ -z $commit || $commit == \#* ]] && continue
    parent=$(git rev-parse "$commit^")
    git diff --unified=0 "$parent" "$commit" -- apps/osl-hub/src/lib.rs | \
      grep -Eq "^\\+pub mod ${module}[ ;{]" || {
        echo "TASK7001_FAIL map-replay old-declaration=$module commit=$commit" >&2; exit 1;
      }
    grep -Eq "^pub mod ${module}[ ;{]" "apps/osl-hub/src/$destination" || {
      echo "TASK7001_FAIL map-replay destination=$destination module=$module" >&2; exit 1;
    }
    count=$((count + 1))
  done < tools/task-7001-check/replay.tsv
  [[ $count -ge 15 ]] || { echo "TASK7001_FAIL map-replay commits=$count" >&2; exit 1; }
  echo "TASK7001_MAP_REPLAY commits=$count"
}

two_lane_measurement() {
  local old new old_status new_status old_conflicts new_conflicts
  old=$(mktemp -d); new=$(mktemp -d)
  trap 'rm -rf "$old" "$new"' RETURN
  for fixture in "$old" "$new"; do
    git -C "$fixture" init -q
    git -C "$fixture" config user.email task7001@osl.invalid
    git -C "$fixture" config user.name task7001
    mkdir -p "$fixture/src"
  done
  # Both fixtures use actual lane source files; the old fixture is a genuine
  # flat historical lib.rs, while the new one has independent area files.
  git show fef8a84db^:apps/osl-hub/src/lib.rs > "$old/src/lib.rs"
  git -C "$old" add src && git -C "$old" commit -qm base
  git -C "$old" checkout -qb invitation-lane
  git show db3016906621d4fd5e4525f099bf4a5833251042:apps/osl-hub/src/place_membership_invites.rs > "$old/src/place_membership_invites.rs"
  printf '\npub mod place_membership_invites;\n' >> "$old/src/lib.rs"
  git -C "$old" add src && git -C "$old" commit -qm invitation-lane
  git -C "$old" checkout -q master && git -C "$old" checkout -qb archive-lane
  git show 0d177d2e2d95d99145632ade5534ce266a31d797:apps/osl-hub/src/protected_archive.rs > "$old/src/protected_archive.rs"
  printf '\npub mod protected_archive;\n' >> "$old/src/lib.rs"
  git -C "$old" add src && git -C "$old" commit -qm archive-lane
  git -C "$old" checkout -q invitation-lane
  set +e; git -C "$old" merge --no-commit archive-lane >/dev/null 2>&1; old_status=$?; set -e
  old_conflicts=$(git -C "$old" diff --name-only --diff-filter=U | wc -l)
  git -C "$old" merge --abort >/dev/null 2>&1 || true

  printf '%s\n' 'include!("module_declarations_messaging_and_hosted.rs");' 'include!("module_declarations_protected_and_scrub.rs");' > "$new/src/lib.rs"
  : > "$new/src/module_declarations_messaging_and_hosted.rs"
  : > "$new/src/module_declarations_protected_and_scrub.rs"
  git -C "$new" add src && git -C "$new" commit -qm base
  git -C "$new" checkout -qb invitation-lane
  git show db3016906621d4fd5e4525f099bf4a5833251042:apps/osl-hub/src/place_membership_invites.rs > "$new/src/place_membership_invites.rs"
  printf 'pub mod place_membership_invites;\n' >> "$new/src/module_declarations_messaging_and_hosted.rs"
  git -C "$new" add src && git -C "$new" commit -qm invitation-lane
  git -C "$new" checkout -q master && git -C "$new" checkout -qb archive-lane
  git show 0d177d2e2d95d99145632ade5534ce266a31d797:apps/osl-hub/src/protected_archive.rs > "$new/src/protected_archive.rs"
  printf 'pub mod protected_archive;\n' >> "$new/src/module_declarations_protected_and_scrub.rs"
  git -C "$new" add src && git -C "$new" commit -qm archive-lane
  git -C "$new" checkout -q invitation-lane
  set +e; git -C "$new" merge --no-commit archive-lane >/dev/null 2>&1; new_status=$?; set -e
  new_conflicts=$(git -C "$new" diff --name-only --diff-filter=U | wc -l)
  [[ $old_status -ne 0 && $old_conflicts -ge 1 && $new_status -eq 0 && $new_conflicts -eq 0 ]] || {
    echo "TASK7001_FAIL two-lane old_exit=$old_status old_conflicts=$old_conflicts new_exit=$new_status new_conflicts=$new_conflicts" >&2; exit 1;
  }
  echo "TASK7001_TWO_LANE old_conflicting_paths=$old_conflicts new_conflicting_paths=$new_conflicts"
}

measure_heavy_lane() {
  local heavy=f4c9061f95c85d6fccc5847753ffc971cf0abd1b parent hunks added retained=0 module
  parent=$(git rev-parse "$heavy^")
  hunks=$(git diff --unified=0 "$parent" "$heavy" -- apps/osl-hub/src/lib.rs | grep -c '^@@')
  added=$(git diff --unified=0 "$parent" "$heavy" -- apps/osl-hub/src/lib.rs | sed -nE 's/^\+pub mod ([A-Za-z_][A-Za-z0-9_]*).*/\1/p' | sort -u)
  while read -r module; do
    [[ -z $module ]] && continue
    if grep -q "^pub mod ${module}[ ;{]" apps/osl-hub/src/module_declarations_*.rs; then retained=$((retained + 1)); fi
  done <<< "$added"
  [[ $hunks -ge 20 && $retained -ge 1 ]] || { echo "TASK7001_FAIL heavy-subset-hunks=$hunks retained=$retained" >&2; exit 1; }
  echo "TASK7001_HEAVY_SUBSET commit=$heavy hunks=$hunks lane_added_declarations=$(printf '%s\n' "$added" | sed '/^$/d' | wc -l) strict_subset_declarations_retained=$retained"
}

run_cargo_check
compare_public_items
replay_map
two_lane_measurement
measure_heavy_lane
