#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_ROOT="${TASK6577_SOURCE_ROOT:-$ROOT}"
EXPECTED_TARGET="/mnt/d/osl-lane-targets/c"
POLICY="$ROOT/docs/evidence/task-6576/frozen-measurement-policy.json"
MEASUREMENT="${TASK6576_MEASUREMENT_ARTIFACT:-$ROOT/docs/evidence/task-6576/measurement.json}"

if [[ "${CARGO_TARGET_DIR:-}" != "$EXPECTED_TARGET" ]]; then
  echo "TASK6577_CHECK_EXIT=1 target CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-absent} expected=$EXPECTED_TARGET" >&2
  exit 1
fi

inventory_output="$(mktemp)"
test_output="$(mktemp)"
verify_output="$(mktemp)"
observation="$(mktemp)"
trap 'rm -f "$inventory_output" "$test_output" "$verify_output" "$observation"' EXIT

scan_roots=()
for candidate in apps crates keyserver-cf cipher-store-cf; do
  if [[ -e "$SOURCE_ROOT/$candidate" ]]; then
    scan_roots+=("$SOURCE_ROOT/$candidate")
  fi
done
if [[ "${#scan_roots[@]}" -eq 0 ]]; then
  echo "TASK6577_CHECK_EXIT=1 attack=cap observed_size=absent source_root=$SOURCE_ROOT" >&2
  exit 1
fi

set +e
rg -n --pcre2 \
  -g '*.rs' -g '*.ts' -g '*.tsx' -g '*.js' -g '*.mjs' -g '*.json' -g '*.sql' -g '*.toml' \
  -g '!**/*.test.*' -g '!**/tests/**' -g '!**/screenshots/**' -g '!**/target/**' \
  '(?ix)(?:
      MAX_(?:ENCLAVE|SPACE|SERVER)[A-Z_]*(?:MEMBERS|MEMBER_COUNT)
    | MAX_MEMBERS_PER_(?:ENCLAVE|SPACE|SERVER)
    | (?:member_osl_user_ids|enclave_members|space_members|server_members|memberCount|member_count)
        (?:\.len\(\)|\.length)?\s*(?:>|>=)\s*[1-9][0-9_]*
    | (?:member|roster)\s+(?:ceiling|cap)
    | roster\s+limit
    | larger\s+rosters\s+are\s+unsupported
    | enclave[^\n]{0,80}(?:at\s+most|up\s+to)\s+[1-9][0-9_]*\s+members
  )' \
  "${scan_roots[@]}" >"$inventory_output" 2>&1
inventory_status=$?
set -e
if [[ "$inventory_status" -eq 0 ]]; then
  cat "$inventory_output" >&2
  echo "TASK6577_CHECK_EXIT=1 attack=cap observed_size=shipping-inventory cap_source_present=true" >&2
  exit 1
fi
if [[ "$inventory_status" -ne 1 ]]; then
  cat "$inventory_output" >&2
  echo "TASK6577_CHECK_EXIT=1 attack=cap inventory_command_exit=$inventory_status" >&2
  exit 1
fi
echo "TASK6577_INVENTORY member_cap_or_reject_above_n=0"

if [[ ! -f "$MEASUREMENT" ]]; then
  echo "TASK6577_CHECK_EXIT=1 attack=estimate measurement_record=$MEASUREMENT basis=absent" >&2
  exit 1
fi
if ! python3 "$ROOT/scripts/task-6576-measurement.py" verify \
  --policy "$POLICY" \
  --artifact "$MEASUREMENT" \
  --allow-provisional-profile >"$verify_output" 2>&1; then
  cat "$verify_output" >&2
  echo "TASK6577_CHECK_EXIT=1 attack=estimate measurement_record=$MEASUREMENT verification=failed" >&2
  exit 1
fi
cat "$verify_output"

read -r measured_n far_above_n < <(python3 - "$MEASUREMENT" <<'PY'
import json
import sys
record = json.load(open(sys.argv[1], encoding="utf-8"))
print(record.get("first_non_prompt_n") or record.get("provisional_n_on_observed_host"), record.get("far_above_n"))
PY
)
if [[ ! "$measured_n" =~ ^[2-9][0-9]*$ || ! "$far_above_n" =~ ^[1-9][0-9]*$ ]]; then
  echo "TASK6577_CHECK_EXIT=1 attack=measurement_record measurement_record=$MEASUREMENT N=$measured_n far_above_n=$far_above_n" >&2
  exit 1
fi

set +e
(
  cd "$ROOT"
  export TASK6576_MEASURED_N="$measured_n"
  export TASK6576_FAR_ABOVE_N="$far_above_n"
  cargo test -p ipc --test task_6576_enclave_removal -- --nocapture --test-threads=1
) >"$test_output" 2>&1
test_status=$?
set -e
cat "$test_output"
if [[ "$test_status" -ne 0 ]]; then
  echo "TASK6577_CHECK_EXIT=1 attack=runtime focused_test_exit=$test_status N=$measured_n" >&2
  exit 1
fi

python3 "$ROOT/scripts/task-6577-proof.py" build-verify \
  --measurement "$MEASUREMENT" \
  --transcript "$test_output" \
  --output "$observation"

echo "TASK6577_CHECK_EXIT=0 sizes=$((measured_n - 1)),$measured_n,$far_above_n N=$measured_n cap=0 truthful_progress=true post_success_removed_reads=0 discarded=true"
