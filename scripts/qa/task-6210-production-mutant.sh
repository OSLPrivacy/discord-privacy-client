#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
expected_target="/mnt/d/osl-lane-targets/c"
if [[ "${CARGO_TARGET_DIR:-}" != "$expected_target" ]]; then
  echo "TASK6210_MUTANT target mismatch expected=$expected_target actual=${CARGO_TARGET_DIR:-unset}" >&2
  exit 2
fi

campaign_root="$(mktemp -d /tmp/task-6210-mutant.XXXXXX)"
trap 'rm -rf -- "$campaign_root"' EXIT
mkdir -p \
  "$campaign_root/apps/osl-hub/src" \
  "$campaign_root/apps/osl-hub/tests" \
  "$campaign_root/tools/task-6210-scrub-rebinding/src" \
  "$campaign_root/tools/task-6210-scrub-rebinding/tests"

production="$repo_root/apps/osl-hub/src/scrub_account_rebinding.rs"
observer="$repo_root/apps/osl-hub/tests/task_6210_scrub_account_rebinding.rs"
production_before="$(sha256sum "$production" | awk '{print $1}')"
observer_before="$(sha256sum "$observer" | awk '{print $1}')"

cp "$production" "$campaign_root/apps/osl-hub/src/"
cp "$observer" "$campaign_root/apps/osl-hub/tests/"
cp "$repo_root/tools/task-6210-scrub-rebinding/Cargo.toml" "$campaign_root/tools/task-6210-scrub-rebinding/"
cp "$repo_root/tools/task-6210-scrub-rebinding/Cargo.lock" "$campaign_root/tools/task-6210-scrub-rebinding/"
cp "$repo_root/tools/task-6210-scrub-rebinding/src/lib.rs" "$campaign_root/tools/task-6210-scrub-rebinding/src/"
cp "$repo_root/tools/task-6210-scrub-rebinding/tests/task_6210_scrub_account_rebinding.rs" \
  "$campaign_root/tools/task-6210-scrub-rebinding/tests/"

mutant="$campaign_root/apps/osl-hub/src/scrub_account_rebinding.rs"
perl -0pi -e 's/if self\.selected_account\.as_ref\(\) != Some\(&approved\)\s*\|\| approved\.provider_source != live\.provider_source\s*\|\| approved\.stable_account_id != live\.stable_account_id\s*\{/if false {/' "$mutant"

if [[ "$(sha256sum "$mutant" | awk '{print $1}')" == "$production_before" ]]; then
  echo "TASK6210_MUTANT omitted production cache-consent mutation" >&2
  exit 2
fi
if [[ "$(sha256sum "$campaign_root/apps/osl-hub/tests/task_6210_scrub_account_rebinding.rs" | awk '{print $1}')" != "$observer_before" ]]; then
  echo "TASK6210_MUTANT observer changed" >&2
  exit 2
fi

log="$campaign_root/mutant.log"
set +e
RUSTC_WRAPPER= CARGO_BUILD_JOBS=1 cargo test \
  --manifest-path "$campaign_root/tools/task-6210-scrub-rebinding/Cargo.toml" \
  -p task-6210-scrub-rebinding \
  --test task_6210_scrub_account_rebinding \
  task_6210_production_guard_mutant_observer_covers_every_source_and_mode \
  -- --test-threads=1 --nocapture >"$log" 2>&1
cargo_status=$?
set -e

if [[ "$cargo_status" -ne 101 ]]; then
  sed -n '1,220p' "$log" >&2
  echo "TASK6210_MUTANT expected Cargo observer exit 101 actual=$cargo_status" >&2
  exit 2
fi
if ! grep -Fq 'TASK6210_PRODUCTION_MUTANT first_B_bound_action=discord/Discovery/list/provider-stable-account-B-6210 mutant_B_actions=34' "$log"; then
  sed -n '1,220p' "$log" >&2
  echo "TASK6210_MUTANT did not observe all 34 B-bound source/mode actions" >&2
  exit 2
fi
if [[ "$(sha256sum "$production" | awk '{print $1}')" != "$production_before" ]] \
  || [[ "$(sha256sum "$observer" | awk '{print $1}')" != "$observer_before" ]]; then
  echo "TASK6210_MUTANT changed the real production source or observer" >&2
  exit 2
fi

grep -F 'TASK6210_PRODUCTION_MUTANT' "$log" >&2
echo "TASK6210_MUTANT_GATE exit=1 cargo_child_exit=101 cells=34 first_B_bound_action=discord/Discovery/list/provider-stable-account-B-6210 mutant_B_actions=34 UI_labels_unchanged=true expected_sets_unchanged=true local_output_suppressed=true deployment_discarded=1" >&2
exit 1
