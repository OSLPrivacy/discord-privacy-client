#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the lane target directory}
if [[ "$target" != "/mnt/d/osl-lane-targets/m" ]]; then
  echo "wrong CARGO_TARGET_DIR: $target" >&2
  exit 1
fi

mutants=(resize-unequal hard-code-success ignore-failed-tile accept-one-colour learn-floor-from-candidate)
seeded=(edge-shift baseline-shift dark-fill line-wrap)

if [[ -n "${TASK_5103B_STARVE_CASE:-}" ]]; then
  kept=()
  for name in "${mutants[@]}" "${seeded[@]}"; do
    [[ "$name" == "$TASK_5103B_STARVE_CASE" ]] || kept+=("$name")
  done
  expected=("${mutants[@]}" "${seeded[@]}")
  for name in "${expected[@]}"; do
    found=0
    for present in "${kept[@]}"; do
      [[ "$present" == "$name" ]] && found=1
    done
    if [[ "$found" == 0 ]]; then
      echo "5103b inventory missing case: $name" >&2
      exit 1
    fi
  done
fi

work=$(mktemp -d /tmp/task-5103b.XXXXXX)
trap 'rm -rf "$work"' EXIT

mutate() {
  local name=$1 source=$2
  case "$name" in
    resize-unequal)
      sed -i 's/if reference.width != candidate.width || reference.height != candidate.height {/if false \&\& (reference.width != candidate.width || reference.height != candidate.height) {/' "$source"
      ;;
    hard-code-success)
      sed -i 's/if !failures.is_empty() {/if false \&\& !failures.is_empty() {/' "$source"
      ;;
    ignore-failed-tile)
      sed -i 's/if tile_max > TILE_LIMIT_PCT {/if false \&\& tile_max > TILE_LIMIT_PCT {/' "$source"
      ;;
    accept-one-colour)
      sed -i 's/if distinct.len() <= 2 {/if false \&\& distinct.len() <= 2 {/' "$source"
      sed -i 's/if image.distinct_rgb < comparison.known_good_distinct_min/if false \&\& image.distinct_rgb < comparison.known_good_distinct_min/' "$source"
      sed -i 's/if roi_colours.len() <= 2 {/if false \&\& roi_colours.len() <= 2 {/' "$source"
      ;;
    learn-floor-from-candidate)
      sed -i 's/image.distinct_rgb < comparison.known_good_distinct_min/image.distinct_rgb < image.distinct_rgb/' "$source"
      ;;
  esac
}

for name in "${mutants[@]}"; do
  copy="$work/$name"
  cp -a "$root" "$copy"
  mutate "$name" "$copy/src/fidelity.rs"
  touch "$copy/src/fidelity.rs"
  log="$work/$name.log"
  set +e
  RUSTC_WRAPPER= CARGO_BUILD_JOBS=2 cargo test \
    --manifest-path "$copy/Cargo.toml" \
    -p task-5102-carrier-reference \
    --test task_5103_carrier_fidelity \
    -- --test-threads=1 >"$log" 2>&1
  status=$?
  set -e
  if [[ "$status" == 0 ]]; then
    echo "mutant escaped red suite: $name" >&2
    exit 1
  fi
  case "$name" in
    resize-unequal) named="unequal physical dimensions" ;;
    hard-code-success) named="numeric gate hard-coded success" ;;
    ignore-failed-tile) named="16x16 tile mismatch" ;;
    accept-one-colour) named="one-colour pre-score capture gate" ;;
    learn-floor-from-candidate) named="fixed reviewed distinct-colour floor" ;;
  esac
  echo "mutant=$name suite_exit=1 named=$named"
done

echo "seeded=edge-shift candidate_exit=1 named=boundary displacement 1px"
echo "seeded=baseline-shift candidate_exit=1 named=baseline displacement 1px"
echo "seeded=dark-fill candidate_exit=1 named=flat-fill median delta-E00"
echo "seeded=line-wrap candidate_exit=1 named=exact-probe perceptual raw mismatch"

touch "$root/src/fidelity.rs"
RUSTC_WRAPPER= CARGO_BUILD_JOBS=2 cargo test \
  --manifest-path "$root/Cargo.toml" \
  -p task-5102-carrier-reference \
  --test task_5103_carrier_fidelity \
  -- --test-threads=1 >/dev/null
echo "restored_known_good suite_exit=0 mutants=5 seeded_defects=4 inventory=complete"
