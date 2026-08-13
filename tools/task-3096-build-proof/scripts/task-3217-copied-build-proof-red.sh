#!/usr/bin/env bash
set -euo pipefail

readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
readonly crate_dir="$repo_root/tools/task-3096-build-proof"
readonly source_file="$crate_dir/src/lib.rs"
readonly test_target="task_3217_copied_build_proof_replay"
readonly expected_target_dir="/mnt/d/osl-lane-targets/i"

if [[ "${CARGO_TARGET_DIR:-}" != "$expected_target_dir" ]]; then
  printf 'TASK3217_ERROR CARGO_TARGET_DIR must be %s, got %s\n' "$expected_target_dir" "${CARGO_TARGET_DIR:-unset}" >&2
  exit 2
fi

backup_file="$(mktemp)"
restore_source() {
  cp "$backup_file" "$source_file"
  rm -f "$backup_file"
}
trap restore_source EXIT
cp "$source_file" "$backup_file"

# This is deliberately a source-level mutant of the shipping device-binding
# guard, not a fixture or expected-answer mutation.
perl -0pi -e 's/if signed\.proof\.device_id != observed_device_id \{\n        return BuildProofAnswer::CannotTell\(CannotTellReason::DifferentDevice\);\n    \}/if false {\n        return BuildProofAnswer::CannotTell(CannotTellReason::DifferentDevice);\n    }/' "$source_file"
if cmp -s "$backup_file" "$source_file"; then
  printf 'TASK3217_ERROR shipping device-binding mutation did not apply\n' >&2
  exit 2
fi

set +e
cd "$repo_root"
RUSTC_WRAPPER= cargo test -j 1 --manifest-path "$crate_dir/Cargo.toml" -p task-3096-build-proof --test "$test_target" -- --test-threads=1 --nocapture
mutant_exit=$?
set -e
if [[ "$mutant_exit" -eq 0 ]]; then
  printf 'TASK3217_ERROR mutant_exit=%s expected=nonzero forbidden_state=unmodified\n' "$mutant_exit" >&2
  exit 1
fi
printf 'TASK3217_MUTANT protection=device-binding source_mutation=removed exit=%s forbidden_external_state=unmodified\n' "$mutant_exit"

restore_source
trap - EXIT

RUSTC_WRAPPER= cargo test -j 1 --manifest-path "$crate_dir/Cargo.toml" -p task-3096-build-proof --test "$test_target" -- --test-threads=1 --nocapture
printf 'TASK3217_RESTORED protection=device-binding source_restored=1 acceptance=pass\n'
