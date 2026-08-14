#!/usr/bin/env bash
set -euo pipefail

revision=${1:-}
if [[ -z "$revision" ]]; then
  echo "usage: $0 <exact-git-revision>" >&2
  exit 64
fi

actual_revision=$(git rev-parse HEAD)
expected_revision=$(git rev-parse "$revision^{commit}")
if [[ "$actual_revision" != "$expected_revision" ]]; then
  echo "TASK0334A_MUTATION revision mismatch expected=$expected_revision actual=$actual_revision" >&2
  exit 65
fi
if [[ "${CARGO_TARGET_DIR:-}" != "/mnt/d/osl-lane-targets/c" ]]; then
  echo "TASK0334A_MUTATION wrong CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-unset}" >&2
  exit 66
fi

source_file=apps/osl-hub/src/account_recovery.rs
test_file=apps/osl-hub/tests/task_0334a_setup_completion_matrix.rs
backup=$(mktemp)
output=$(mktemp)
cp "$source_file" "$backup"
source_sha256=$(sha256sum "$backup" | awk '{print $1}')
test_sha256=$(sha256sum "$test_file" | awk '{print $1}')
revision_source_sha256=$(git show "$expected_revision:$source_file" | sha256sum | awk '{print $1}')
revision_test_sha256=$(git show "$expected_revision:$test_file" | sha256sum | awk '{print $1}')
if [[ "$source_sha256" != "$revision_source_sha256" || "$test_sha256" != "$revision_test_sha256" ]]; then
  echo "TASK0334A_MUTATION revision content mismatch source=$source_sha256/$revision_source_sha256 test=$test_sha256/$revision_test_sha256" >&2
  rm -f "$backup" "$output"
  exit 67
fi

restore() {
  cp "$backup" "$source_file"
}
cleanup() {
  restore
  rm -f "$backup" "$output"
}
trap cleanup EXIT

run_red() {
  local mutation=$1
  local expected_route=$2
  run_matrix_once() {
    if CARGO_BUILD_JOBS=1 RUSTC_WRAPPER= cargo test --manifest-path apps/osl-hub/Cargo.toml -p osl-hub --no-default-features --features core \
      --test task_0334a_setup_completion_matrix -- --test-threads=1 --nocapture \
      >"$output" 2>&1; then
      return 0
    fi
    return 1
  }
  set +e
  run_matrix_once
  local status=$?
  set -e
  if [[ $status -eq 0 ]]; then
    echo "TASK0334A_MUTATION mutation=$mutation unexpectedly_green=true" >&2
    exit 1
  fi
  if ! grep -Fq "$expected_route" "$output"; then
    tail -n 80 "$output" >&2
    echo "TASK0334A_MUTATION mutation=$mutation missing_route=$expected_route" >&2
    exit 1
  fi
  echo "TASK0334A_MUTATION mutation=$mutation exit=$status named_route=$expected_route red=true"
  restore
}

perl -0pi -e 's/Some\(RecoverySetupBranch::NoRecoverySecret\) => \{.*?\n        \}/Some(RecoverySetupBranch::NoRecoverySecret) => false,/s' "$source_file"
run_red confirmation-only no-secret/app-choice

perl -0pi -e 's/None => false,/None => true,/' "$source_file"
run_red missing-counts-as-no-secret missing/app-choice

perl -0pi -e 's/Some\(RecoverySetupBranch::RecoveryWords\) => \{.*?\n        \}/Some(RecoverySetupBranch::RecoveryWords) => true,/s' "$source_file"
run_red words-bypass-confirmation words-unconfirmed/app-choice

CARGO_BUILD_JOBS=1 RUSTC_WRAPPER= cargo test --manifest-path apps/osl-hub/Cargo.toml -p osl-hub --no-default-features --features core \
  --test task_0334a_setup_completion_matrix -- --test-threads=1 --nocapture

restored_sha256=$(sha256sum "$source_file" | awk '{print $1}')
restored_test_sha256=$(sha256sum "$test_file" | awk '{print $1}')
if [[ "$source_sha256" != "$restored_sha256" || "$test_sha256" != "$restored_test_sha256" ]]; then
  echo "TASK0334A_MUTATION restoration hash mismatch" >&2
  exit 1
fi
echo "TASK0334A_MUTATIONS revision=$actual_revision planned=3 executed=3 omitted=0 red=3 restoration_green=1 revision_bound_files=2 source_sha256=$source_sha256 restored_sha256=$restored_sha256 test_sha256=$test_sha256 restored_test_sha256=$restored_test_sha256"
