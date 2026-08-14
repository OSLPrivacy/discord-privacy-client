#!/usr/bin/env bash
set -euo pipefail

# TASK 5646b: execute the unchanged TASK 5646 production gate against one
# mutation per disposable source/evidence tree.  The source ref is explicit so
# this lane can test the gate commit even before the integration branch merges
# it into this worktree.

readonly REPO_ROOT="$(git rev-parse --show-toplevel)"
readonly SOURCE_REF="${TASK5646_SOURCE_REF:-7076836d690806d025b70bf99c1d0b31195b0d4f}"
readonly TARGET_DIR="/mnt/d/osl-lane-targets/c"
readonly INVENTORY_HASH="26416383600ed91d13709e467a5f73fbae67935c6ae8fa806cf86b256e22957a"
readonly FIX_EPOCH="1786455094"
readonly EXPECTED_MUTATIONS=8

scratch="$(mktemp -d /tmp/task-5646b.XXXXXX)"
trap 'rm -rf -- "$scratch"' EXIT

case_count=0
mutation_count=0
discarded_count=0

materialize_case() {
  local name="$1"
  local case_root="$scratch/$name"
  mkdir -p "$case_root"
  git -C "$REPO_ROOT" archive "$SOURCE_REF" | tar -x -C "$case_root"

  local evidence_root="$case_root/.task5646b-evidence"
  mkdir -p "$evidence_root"
  local task
  while IFS= read -r task; do
    {
      printf 'capture_kind=outside-process\n'
      printf 'account_kind=real-service-account\n'
      printf 'command_kind=compiled-shipping-command\n'
      printf 'inventory_hash=%s\n' "$INVENTORY_HASH"
      printf 'fixture_count=0\n'
    } > "$evidence_root/$task.md"
    touch -d "@$((FIX_EPOCH + 1))" "$evidence_root/$task.md"
  done < <(
    sed -n '/"tasks"/,$p' "$case_root/data/mail-rerun-policy-v1.json" |
      rg -o '"[0-9]+[bc]?"' |
      tr -d '"'
  )

  case_count=$((case_count + 1))
  materialized_root="$case_root"
}

discard_case() {
  local case_root="$1"
  rm -rf -- "$case_root"
  discarded_count=$((discarded_count + 1))
}

run_gate() {
  local name="$1"
  local case_root="$2"
  local expected_exit="$3"
  local expected_pattern="$4"
  local output="$case_root/task5646.output"
  local evidence_root="$case_root/.task5646b-evidence"

  set +e
  (
    cd "$case_root"
    CARGO_TARGET_DIR="$TARGET_DIR" RUSTC_WRAPPER= cargo run \
      --offline -q -j 1 -p mail-service-inventory \
      --bin task_5646_mail_inventory -- \
      --evidence-root "$evidence_root"
  ) >"$output" 2>&1
  local actual_exit=$?
  set -e

  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    printf 'TASK5646B_HARNESS_FAILURE case=%s expected_exit=%s actual_exit=%s\n' \
      "$name" "$expected_exit" "$actual_exit" >&2
    sed -n '1,220p' "$output" >&2
    exit 1
  fi
  if ! rg -q -- "$expected_pattern" "$output"; then
    printf 'TASK5646B_HARNESS_FAILURE case=%s missing_pattern=%s\n' \
      "$name" "$expected_pattern" >&2
    sed -n '1,220p' "$output" >&2
    exit 1
  fi
  printf 'TASK5646B_CASE=%s EXIT=%s MATCH=%s\n' "$name" "$actual_exit" "$expected_pattern"
  rg 'TASK5646_(CONFIGURATION_SERVICES|RUNTIME_SERVICES|SERVICES|HALVES|MAIL_GRAPH_CYCLES|FAILURE|RESULT)=' \
    "$output" | tail -n 8 || true
}

mutate_omit_runtime_service() {
  local file="$1/apps/osl-hub/src/mail_service_inventory_runtime.rs"
  perl -0pi -e 's/    \(\n        Surface::Gmail,\n        ServiceIdentity::GmailWeb,\n        EntryPointKind::Web,\n    \),\n//' "$file"
  ! rg -q 'ServiceIdentity::GmailWeb' "$file"
}

mutate_merge_outlook_identities() {
  local file="$1/apps/osl-hub/src/mail_service_inventory_runtime.rs"
  perl -0pi -e 's/    \(\n        Surface::OutlookDesktop,\n        ServiceIdentity::OutlookDesktop,\n        EntryPointKind::Native,\n    \),\n//' "$file"
  ! rg -q 'ServiceIdentity::OutlookDesktop' "$file"
}

mutate_tuta_runtime_only() {
  local library="$1/crates/mail-service-inventory/src/lib.rs"
  local runtime="$1/apps/osl-hub/src/mail_service_inventory_runtime.rs"
  perl -0pi -e 's/    TutaWeb,\n/    TutaWeb,\n    TutaRuntimeOnly,\n/; s/            Self::TutaWeb => "tuta-web",\n/            Self::TutaWeb => "tuta-web",\n            Self::TutaRuntimeOnly => "tuta-web-runtime-only",\n/' "$library"
  perl -0pi -e 's/    \(Surface::Tuta, ServiceIdentity::TutaWeb, EntryPointKind::Web\),\n/    (Surface::Tuta, ServiceIdentity::TutaWeb, EntryPointKind::Web),\n    (\n        Surface::Tuta,\n        ServiceIdentity::TutaRuntimeOnly,\n        EntryPointKind::Web,\n    ),\n/' "$runtime"
  rg -q 'Self::TutaRuntimeOnly => "tuta-web-runtime-only"' "$library"
  rg -q 'ServiceIdentity::TutaRuntimeOnly' "$runtime"
}

mutate_literal_count() {
  local root="$1"
  local count="$2"
  local file="$root/data/mail-task-consumers-v1.json"
  perl -0pi -e "s/  \"inventory_version\":/  \"services\": $count,\n  \"inventory_version\":/" "$file"
  rg -q "\"services\": $count" "$file"
}

mutate_4323_4325c_cycle() {
  local file="$1/data/mail-task-graph-v1.json"
  perl -0pi -e 's/    \{ "from": "4325", "to": "4325b" \}\n/    { "from": "4325", "to": "4325b" },\n    { "from": "4323", "to": "4325c" }\n/' "$file"
  rg -q '"from": "4323", "to": "4325c"' "$file"
}

mutate_fixture_reader_upload() {
  local file="$1/.task5646b-evidence/4322b.md"
  perl -0pi -e 's/capture_kind=outside-process/capture_kind=fixture-reader-upload/' "$file"
  rg -q '^capture_kind=fixture-reader-upload$' "$file"
}

mutate_stale_descendant() {
  local file="$1/.task5646b-evidence/4507b.md"
  touch -d "@$FIX_EPOCH" "$file"
  [[ "$(stat -c %Y "$file")" == "$FIX_EPOCH" ]]
}

run_mutation() {
  local name="$1"
  local expected_pattern="$2"
  local mutator="$3"
  shift 3
  local root
  materialize_case "$name"
  root="$materialized_root"
  "$mutator" "$root" "$@"
  mutation_count=$((mutation_count + 1))
  run_gate "$name" "$root" 1 "$expected_pattern"
  discard_case "$root"
}

git -C "$REPO_ROOT" cat-file -e "$SOURCE_REF:crates/mail-service-inventory/src/bin/task_5646_mail_inventory.rs"
printf 'TASK5646B_SOURCE_REF=%s\n' "$SOURCE_REF"
printf 'TASK5646B_CARGO_TARGET_DIR=%s\n' "$TARGET_DIR"

materialize_case baseline-independent-real-service-shape
baseline_root="$materialized_root"
run_gate baseline-independent-real-service-shape "$baseline_root" 0 'TASK5646_RESULT=PASS'
discard_case "$baseline_root"

run_mutation omit-runtime-gmail \
  'runtime inventory is missing service gmail-web' mutate_omit_runtime_service
run_mutation merge-outlook-identities \
  'runtime inventory squashed distinct Outlook identities into outlook-web' mutate_merge_outlook_identities
run_mutation tuta-runtime-only \
  'runtime inventory has extra service tuta-web-runtime-only' mutate_tuta_runtime_only
run_mutation literal-count-10 \
  'hard-codes contradictory count.*10' mutate_literal_count 10
run_mutation literal-count-11 \
  'hard-codes contradictory count.*11' mutate_literal_count 11
run_mutation cycle-4323-4325c \
  'mail graph cycle edge 4323 -> 4325c' mutate_4323_4325c_cycle
run_mutation fixture-reader-upload \
  'task 4322b evidence is missing acceptance token capture_kind=outside-process' mutate_fixture_reader_upload
run_mutation stale-descendant \
  'stale checked descendant task 4507b' mutate_stale_descendant

materialize_case restored-independent-real-service-shape
restored_root="$materialized_root"
run_gate restored-independent-real-service-shape "$restored_root" 0 'TASK5646_RESULT=PASS'
discard_case "$restored_root"

empty_set_count=0
if [[ "$EXPECTED_MUTATIONS" -eq 0 ]]; then
  empty_set_count=1
fi
omitted_mutation_count=$((EXPECTED_MUTATIONS - mutation_count))
remaining_copy_count="$(find "$scratch" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"

# Anti-cheat scores are all-or-nothing.  A fabricated report line cannot name
# any real gate diagnostic; an empty result set and a seven-of-eight result set
# are likewise worth zero qualifying mutations.
sentinel_output="$scratch/planted-report-sentinel.output"
printf 'TASK5646B_REPORT_SENTINEL=pretend-red-proof\n' > "$sentinel_output"
planted_report_sentinel_mutation_score=0
for pattern in \
  'runtime inventory is missing service gmail-web' \
  'runtime inventory squashed distinct Outlook identities into outlook-web' \
  'runtime inventory has extra service tuta-web-runtime-only' \
  'hard-codes contradictory count at $.services: 10' \
  'hard-codes contradictory count at $.services: 11' \
  'mail graph cycle edge 4323 -> 4325c' \
  'task 4322b evidence is missing acceptance token capture_kind=outside-process' \
  'stale checked descendant task 4507b'; do
  if rg -Fq -- "$pattern" "$sentinel_output"; then
    planted_report_sentinel_mutation_score=$((planted_report_sentinel_mutation_score + 1))
  fi
done
empty_set_mutation_score=0
omitted_mutation_score=0
observed_if_one_omitted=$((mutation_count - 1))
if [[ "$observed_if_one_omitted" -eq "$EXPECTED_MUTATIONS" ]]; then
  omitted_mutation_score="$observed_if_one_omitted"
fi

printf 'TASK5646B_PLANNED_MUTATIONS=%s\n' "$EXPECTED_MUTATIONS"
printf 'TASK5646B_EXECUTED_MUTATIONS=%s\n' "$mutation_count"
printf 'TASK5646B_EMPTY_SET_COUNT=%s\n' "$empty_set_count"
printf 'TASK5646B_OMITTED_MUTATION_COUNT=%s\n' "$omitted_mutation_count"
printf 'TASK5646B_PLANTED_REPORT_SENTINEL_MUTATION_SCORE=%s\n' "$planted_report_sentinel_mutation_score"
printf 'TASK5646B_EMPTY_SET_MUTATION_SCORE=%s\n' "$empty_set_mutation_score"
printf 'TASK5646B_OMITTED_MUTATION_SCORE=%s\n' "$omitted_mutation_score"
printf 'TASK5646B_DISCARDED_COPIES=%s\n' "$discarded_count"
printf 'TASK5646B_REMAINING_COPIES=%s\n' "$remaining_copy_count"

[[ "$mutation_count" -eq "$EXPECTED_MUTATIONS" ]]
[[ "$empty_set_count" -eq 0 ]]
[[ "$omitted_mutation_count" -eq 0 ]]
[[ "$planted_report_sentinel_mutation_score" -eq 0 ]]
[[ "$empty_set_mutation_score" -eq 0 ]]
[[ "$omitted_mutation_score" -eq 0 ]]
[[ "$discarded_count" -eq "$case_count" ]]
[[ "$remaining_copy_count" -eq 0 ]]
printf 'TASK5646B_RESULT=PASS\n'
