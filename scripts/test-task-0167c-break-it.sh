#!/usr/bin/env bash
set -euo pipefail

# TASK 0167c: run the unchanged TASK 0167b production-boundary gate against
# six separate, revision-bound throwaway source copies. Only the production
# admission module is mutated; the test, connector registries, supported email
# implementation, and every positive control remain byte-for-byte at SOURCE_REF.

readonly REPO_ROOT="$(git rev-parse --show-toplevel)"
readonly SOURCE_REF="${TASK0167C_SOURCE_REF:-$(git -C "$REPO_ROOT" rev-parse HEAD)}"
readonly TARGET_DIR="/mnt/d/osl-lane-targets/c"
readonly TEST_FILE="crates/ipc/tests/task_0167b_production_kind_admission.rs"
readonly ADMISSION_FILE="crates/ipc/src/production_kind_admission.rs"
readonly EMAIL_FILE="crates/ipc/src/shipping_email.rs"
readonly EXPECTED_MUTATIONS=6

scratch="$(mktemp -d /tmp/task-0167c.XXXXXX)"
trap 'rm -rf -- "$scratch"' EXIT

# The lane contains unrelated, tracked fixes newer than SOURCE_REF which are
# required for `ipc` to compile. Snapshot that build-support overlay once, hash
# it, and apply the identical snapshot to every archive. The three protected
# production/test files are forbidden from the overlay and remain exactly at
# SOURCE_REF; untracked files are never copied.
support_list="$scratch/build-support.paths"
support_tar="$scratch/build-support.tar"
git -C "$REPO_ROOT" diff --name-only -z "$SOURCE_REF" -- . >"$support_list"
if tr '\0' '\n' <"$support_list" | rg -x \
  "$TEST_FILE|$ADMISSION_FILE|$EMAIL_FILE|scripts/test-task-0167c-break-it.sh"; then
  printf 'TASK0167C_HARNESS_FAILURE protected file present in build-support overlay\n' >&2
  exit 1
fi
if git -C "$REPO_ROOT" diff --name-only --diff-filter=D "$SOURCE_REF" -- . | rg -q .; then
  printf 'TASK0167C_HARNESS_FAILURE build-support overlay contains a deletion\n' >&2
  exit 1
fi
tar -C "$REPO_ROOT" --null --files-from="$support_list" -cf "$support_tar"
support_hash="$(sha256sum "$support_tar" | cut -d' ' -f1)"
support_count="$(tr -cd '\0' <"$support_list" | wc -c | tr -d ' ')"

test_hash="$(git -C "$REPO_ROOT" show "$SOURCE_REF:$TEST_FILE" | sha256sum | cut -d' ' -f1)"
admission_hash="$(git -C "$REPO_ROOT" show "$SOURCE_REF:$ADMISSION_FILE" | sha256sum | cut -d' ' -f1)"
email_hash="$(git -C "$REPO_ROOT" show "$SOURCE_REF:$EMAIL_FILE" | sha256sum | cut -d' ' -f1)"

copy_count=0
discarded_count=0
mutation_count=0

hash_file() {
  sha256sum "$1" | cut -d' ' -f1
}

require_hash() {
  local path="$1"
  local expected="$2"
  local label="$3"
  local actual
  actual="$(hash_file "$path")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'TASK0167C_HARNESS_FAILURE hash=%s expected=%s actual=%s\n' \
      "$label" "$expected" "$actual" >&2
    exit 1
  fi
}

materialize_copy() {
  local name="$1"
  local root="$scratch/active-copy"
  [[ ! -e "$root" ]]
  mkdir -p "$root"
  git -C "$REPO_ROOT" archive "$SOURCE_REF" | tar -x -C "$root"
  require_hash "$root/$TEST_FILE" "$test_hash" test
  require_hash "$root/$ADMISSION_FILE" "$admission_hash" admission
  require_hash "$root/$EMAIL_FILE" "$email_hash" shipping_email
  tar -xf "$support_tar" -C "$root"
  require_hash "$root/$TEST_FILE" "$test_hash" test
  require_hash "$root/$ADMISSION_FILE" "$admission_hash" admission
  require_hash "$root/$EMAIL_FILE" "$email_hash" shipping_email
  # A restored archive can otherwise have an older mtime than the preceding
  # mutant at this reused path, allowing Cargo to execute the stale mutant.
  touch "$root/$TEST_FILE" "$root/$ADMISSION_FILE" "$root/$EMAIL_FILE"
  copy_count=$((copy_count + 1))
  materialized_root="$root"
  printf 'TASK0167C_COPY name=%s ordinal=%s source_ref=%s test_sha256=%s admission_sha256=%s email_sha256=%s\n' \
    "$name" "$copy_count" "$SOURCE_REF" "$test_hash" "$admission_hash" "$email_hash"
}

discard_copy() {
  local root="$1"
  rm -rf -- "$root"
  discarded_count=$((discarded_count + 1))
}

require_supported_controls() {
  local output="$1"
  rg -q 'TASK0167B_REGISTRIES .*discord_count=5 .*email_count=2 .*signed_in_discord=native_discord supported_email=gmail' "$output"
  rg -q 'TASK0167B_CONTROLS phase=before discord=direct_message=discord:direct_message:allowed\|group_chat=discord:group_chat:allowed\|server=discord:server:allowed\|server_channel=discord:server_channel:allowed\|thread=discord:thread:allowed email=address=email_address:provider_allowed\|domain=email_domain:provider_allowed' "$output"
}

run_green_gate() {
  local name="$1"
  local root="$2"
  local output="$root/task0167c-$name.output"
  local cargo_exit
  set +e
  (
    cd "$root"
    CARGO_TARGET_DIR="$TARGET_DIR" RUSTC_WRAPPER= cargo test -j 1 -p ipc \
      --test task_0167b_production_kind_admission -- --test-threads=1 --nocapture
  ) >"$output" 2>&1
  cargo_exit=$?
  set -e
  if [[ "$cargo_exit" -ne 0 ]]; then
    printf 'TASK0167C_HARNESS_FAILURE name=%s expected_green=true raw_cargo_exit=%s\n' \
      "$name" "$cargo_exit" >&2
    sed -n '1,260p' "$output" >&2
    tail -n 120 "$output" >&2
    exit 1
  fi
  require_supported_controls "$output"
  rg -q 'TASK0167B_CONTROLS phase=after ' "$output"
  rg -q 'TASK0167B_CONTROL_COUNTS discord_before=5 discord_after=5 email_before=2 email_after=2 normalized=14 rule_lookups=14 allowed_rows=14 pending_rows=0 prompts=0 notices=0 provider_actions=14 output_rows=14' "$output"
  rg -q 'test result: ok. 1 passed; 0 failed; 1 ignored' "$output"
  printf 'TASK0167C_GREEN name=%s exit=0 discord_controls=5 email_controls=2 hostile_helpers=12 downstream_per_hostile=0\n' "$name"
}

run_red_gate() {
  local name="$1"
  local carrier="$2"
  local raw_pattern="$3"
  local first_forbidden="$4"
  local downstream_total="$5"
  local root="$6"
  local output="$root/task0167c-$name.output"
  local cargo_exit
  local forbidden_line
  local observed_raw

  require_hash "$root/$TEST_FILE" "$test_hash" test
  require_hash "$root/$EMAIL_FILE" "$email_hash" shipping_email
  if [[ "$(hash_file "$root/$ADMISSION_FILE")" == "$admission_hash" ]]; then
    printf 'TASK0167C_HARNESS_FAILURE case=%s production mutation did not change admission source\n' "$name" >&2
    exit 1
  fi

  set +e
  (
    cd "$root"
    CARGO_TARGET_DIR="$TARGET_DIR" RUSTC_WRAPPER= cargo test -j 1 -p ipc \
      --test task_0167b_production_kind_admission -- --test-threads=1 --nocapture
  ) >"$output" 2>&1
  cargo_exit=$?
  set -e

  if [[ "$cargo_exit" -eq 0 ]]; then
    printf 'TASK0167C_HARNESS_FAILURE case=%s mutation stayed green\n' "$name" >&2
    sed -n '1,240p' "$output" >&2
    exit 1
  fi
  require_supported_controls "$output"
  forbidden_line="$(rg "TASK0167B_FORBIDDEN carrier=$carrier raw_kind=$raw_pattern first_forbidden=$first_forbidden .*downstream_total=$downstream_total" "$output" | head -n 1)"
  observed_raw="$(sed -E 's/.* raw_kind=([^ ]+) first_forbidden=.*/\1/' <<<"$forbidden_line")"
  rg -q "$carrier hostile $raw_pattern must exit 1" "$output"
  if rg -q 'TASK0167B_CONTROLS phase=after ' "$output"; then
    printf 'TASK0167C_HARNESS_FAILURE case=%s red gate unexpectedly reached after-controls\n' "$name" >&2
    exit 1
  fi

  # cargo test uses 101 for an assertion failure. Normalize the unchanged
  # gate's red result to the build-plan vocabulary while retaining the raw code.
  printf 'TASK0167C_RED case=%s carrier=%s raw_kind=%s first_forbidden=%s gate_exit=1 raw_cargo_exit=%s downstream_total=%s signed_in_discord=native_discord supported_email=gmail controls=5+2 test_unchanged=true\n' \
    "$name" "$carrier" "$observed_raw" "$first_forbidden" "$cargo_exit" "$downstream_total"
}

mutate_unknown_to_default() {
  local root="$1"
  local carrier="$2"
  local registry="$3"
  local file="$root/$ADMISSION_FILE"
  perl -0pi -e "s@    let raw_kind = require_exact_kind\(\"$carrier\", &object, &$registry, observer\)\?;@    let raw_kind = match object.get(\"kind\").and_then(Value::as_str) {\n        Some(candidate) if candidate.starts_with(\"unknown_\") => $registry[0].to_owned(),\n        _ => require_exact_kind(\"$carrier\", \&object, \&$registry, observer)?,\n    };@" "$file"
  rg -q "candidate.starts_with\(\"unknown_\"\) => $registry\[0\]" "$file"
}

mutate_accept_empty() {
  local root="$1"
  local carrier="$2"
  local registry="$3"
  local file="$root/$ADMISSION_FILE"
  perl -0pi -e "s@    let raw_kind = require_exact_kind\(\"$carrier\", &object, &$registry, observer\)\?;@    let raw_kind = match object.get(\"kind\").and_then(Value::as_str) {\n        Some(\"\") => $registry[0].to_owned(),\n        _ => require_exact_kind(\"$carrier\", \&object, \&$registry, observer)?,\n    };@" "$file"
  rg -q "Some\(\"\"\) => $registry\[0\]" "$file"
}

mutate_lookup_before_validation() {
  local root="$1"
  local carrier="$2"
  local registry="$3"
  local key_prefix="$4"
  local file="$root/$ADMISSION_FILE"
  perl -0pi -e "s@    let object = deserialize_object\(\"$carrier\", provider_json, observer\)\?;\n    let raw_kind = require_exact_kind\(\"$carrier\", &object, &$registry, observer\)\?;@    let object = deserialize_object(\"$carrier\", provider_json, observer)?;\n    if let Some(candidate) = object.get(\"kind\").and_then(Value::as_str) {\n        let key = format!(\"$key_prefix{}\", candidate);\n        let _premature_choice = state\n            .app_preferences\n            .lock()\n            .expect(\"app_preferences mutex poisoned\")\n            .auto_whitelist_rules\n            .get(\&key)\n            .copied()\n            .unwrap_or_default();\n        observer.rule_lookups += 1;\n    }\n    let raw_kind = require_exact_kind(\"$carrier\", \&object, \&$registry, observer)?;@" "$file"
  rg -q '_premature_choice' "$file"
  rg -q "format!\(\"$key_prefix\{\}\", candidate\)" "$file"
}

run_mutation() {
  local name="$1"
  local carrier="$2"
  local raw_pattern="$3"
  local first_forbidden="$4"
  local downstream_total="$5"
  local mutator="$6"
  shift 6
  local root
  materialize_copy "$name"
  root="$materialized_root"
  "$mutator" "$root" "$carrier" "$@"
  mutation_count=$((mutation_count + 1))
  run_red_gate "$name" "$carrier" "$raw_pattern" "$first_forbidden" "$downstream_total" "$root"
  discard_copy "$root"
}

git -C "$REPO_ROOT" cat-file -e "$SOURCE_REF^{commit}"
[[ "$SOURCE_REF" =~ ^[0-9a-f]{40}$ ]]
printf 'TASK0167C_SOURCE_REF=%s\n' "$SOURCE_REF"
printf 'TASK0167C_CARGO_TARGET_DIR=%s\n' "$TARGET_DIR"
printf 'TASK0167C_TEST_SHA256=%s\n' "$test_hash"
printf 'TASK0167C_ADMISSION_SHA256=%s\n' "$admission_hash"
printf 'TASK0167C_EMAIL_SHA256=%s\n' "$email_hash"
printf 'TASK0167C_BUILD_SUPPORT_FILES=%s\n' "$support_count"
printf 'TASK0167C_BUILD_SUPPORT_SHA256=%s\n' "$support_hash"

materialize_copy baseline
baseline_root="$materialized_root"
run_green_gate baseline "$baseline_root"
discard_copy "$baseline_root"

run_mutation discord-unknown-default discord 'unknown_[0-9a-f]{32}' normalization 5 \
  mutate_unknown_to_default DISCORD_DECLARED_KINDS
run_mutation discord-empty-accepted discord '<empty>' normalization 5 \
  mutate_accept_empty DISCORD_DECLARED_KINDS
run_mutation discord-premature-lookup discord '<empty>' lookup 1 \
  mutate_lookup_before_validation DISCORD_DECLARED_KINDS 'discord:'
run_mutation email-unknown-default email 'unknown_[0-9a-f]{32}' normalization 5 \
  mutate_unknown_to_default EMAIL_DECLARED_KINDS
run_mutation email-empty-accepted email '<empty>' normalization 5 \
  mutate_accept_empty EMAIL_DECLARED_KINDS
run_mutation email-premature-lookup email '<empty>' lookup 1 \
  mutate_lookup_before_validation EMAIL_DECLARED_KINDS 'email_'

materialize_copy restored
restored_root="$materialized_root"
require_hash "$restored_root/$TEST_FILE" "$test_hash" test
require_hash "$restored_root/$ADMISSION_FILE" "$admission_hash" admission
require_hash "$restored_root/$EMAIL_FILE" "$email_hash" shipping_email
run_green_gate restored "$restored_root"
discard_copy "$restored_root"

remaining_copies="$(find "$scratch" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
omitted_mutations=$((EXPECTED_MUTATIONS - mutation_count))
printf 'TASK0167C_PLANNED_MUTATIONS=%s\n' "$EXPECTED_MUTATIONS"
printf 'TASK0167C_EXECUTED_MUTATIONS=%s\n' "$mutation_count"
printf 'TASK0167C_OMITTED_MUTATIONS=%s\n' "$omitted_mutations"
printf 'TASK0167C_COPIES=%s\n' "$copy_count"
printf 'TASK0167C_DISCARDED_COPIES=%s\n' "$discarded_count"
printf 'TASK0167C_REMAINING_COPIES=%s\n' "$remaining_copies"
[[ "$mutation_count" -eq "$EXPECTED_MUTATIONS" ]]
[[ "$omitted_mutations" -eq 0 ]]
[[ "$discarded_count" -eq "$copy_count" ]]
[[ "$remaining_copies" -eq 0 ]]
printf 'TASK0167C_RESULT=PASS\n'
