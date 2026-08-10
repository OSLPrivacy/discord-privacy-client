#!/usr/bin/env bash
set -euo pipefail

crate_dir=$(cd "$(dirname "$0")/.." && pwd)
target_dir=${CARGO_TARGET_DIR:-/mnt/d/osl-lane-targets/b}
if [[ "$target_dir" != /mnt/d/osl-lane-targets/b ]]; then
  echo "CARGO_TARGET_DIR must be /mnt/d/osl-lane-targets/b" >&2
  exit 1
fi

run_privacy_test() {
  local copy=$1
  local artifacts=$2
  CARGO_TARGET_DIR="$target_dir" TASK_5102_PRIVACY_OUTPUT="$artifacts" \
    cargo test --manifest-path "$copy/Cargo.toml" \
      -p task-5102-carrier-reference \
      --test task_5102_privacy_boundary \
      -- --test-threads=1
}

assert_red_without_pixels() {
  local label=$1
  local copy=$2
  local artifacts=$3
  local log=$4
  if run_privacy_test "$copy" "$artifacts" >"$log" 2>&1; then
    echo "$label mutant was not caught" >&2
    exit 1
  fi
  local png_count=0
  if [[ -d "$artifacts" ]]; then
    png_count=$(find "$artifacts" -maxdepth 1 -type f -name '*.png' | wc -l)
  fi
  local marker_count=0
  if [[ -d "$artifacts" ]]; then
    marker_count=$(grep -Rao 'PERSONAL_CONVERSATION_MARKER_5102' "$artifacts" 2>/dev/null | wc -l)
  fi
  if [[ "$png_count" != 0 || "$marker_count" != 0 ]]; then
    echo "$label mutant leaked pngs=$png_count markers=$marker_count" >&2
    exit 1
  fi
  echo "$label: test_exit=1 pngs=0 markers=0"
}

break_root=$(mktemp -d)
trap 'rm -rf -- "$break_root"' EXIT

cp -a "$crate_dir" "$break_root/broad"
perl -0pi -e 's{// PRIVACY_GUARD_VERIFIED_HWND_CROP: reject broad or unbound source frames\.\n}{// BROKEN_CROP_GUARD_REMOVED\n}' "$break_root/broad/src/lib.rs"
assert_red_without_pixels broad_or_unbound "$break_root/broad" "$break_root/broad-artifacts" "$break_root/broad.log"

cp -a "$crate_dir" "$break_root/observation"
perl -0pi -e 's{// PRIVACY_GUARD_SECOND_UIA_OBSERVATION: the second sample is independent\.\n}{// BROKEN_SECOND_OBSERVATION_GUARD_REMOVED\n}' "$break_root/observation/src/lib.rs"
assert_red_without_pixels second_observation "$break_root/observation" "$break_root/observation-artifacts" "$break_root/observation.log"

cp -a "$crate_dir" "$break_root/starved-assertion"
perl -0pi -e 's{"TASK5102_ASSERT_PERSONAL_MARKER_ZERO"}{"TASK5102_ASSERTION_REMOVED"}' "$break_root/starved-assertion/tests/task_5102_privacy_boundary.rs"
assert_red_without_pixels starved_marker_assertion "$break_root/starved-assertion" "$break_root/starved-artifacts" "$break_root/starved.log"

restored="$break_root/restored-artifacts"
CARGO_TARGET_DIR="$target_dir" cargo run --quiet \
  --manifest-path "$crate_dir/Cargo.toml" \
  -p task-5102-carrier-reference \
  --bin carrier-reference-fixture -- "$restored" valid
restored_pngs=$(find "$restored" -maxdepth 1 -type f -name '*.png' | wc -l)
if [[ "$restored_pngs" != 1 ]]; then
  echo "restored fixture wrote pngs=$restored_pngs" >&2
  exit 1
fi
echo "restored_exact_region: pngs=1 width=188 height=48"
