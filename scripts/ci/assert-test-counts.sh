#!/usr/bin/env bash
set -euo pipefail

# ON SUMMING MULTIPLE SUMMARIES, because it is right in one place and risky in
# another. cargo prints one `test result:` line per test binary, and
# keyserver-cf's `npm test` runs two vitest invocations (310 + 3 = 313), so
# summing is the CORRECT reading for both. But the same behaviour would also
# sum a RETRIED job's duplicate summary and report twice the tests that exist,
# which would let a real drop hide behind a retry. If retries are ever enabled
# for these jobs, this needs to de-duplicate identical summaries rather than
# add them. Recorded here rather than only in the report so it is read by
# whoever changes this function.

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
FLOORS_FILE="$SCRIPT_DIR/test-count-floors.txt"
SELF_TEST_TEMP_DIR=

usage() {
  echo "usage: $0 <suite-name> <file-containing-test-output>" >&2
  echo "       $0 --self-test" >&2
}

error() {
  echo "::error::$*" >&2
  return 1
}

floor_for_suite() {
  local suite=$1

  awk -v wanted="$suite" '
    {
      line = $0
      sub(/[[:space:]]*#.*/, "", line)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      if (line == "") {
        next
      }
      split(line, parts, "=")
      name = parts[1]
      value = parts[2]
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", name)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
      if (name == wanted) {
        print value
        found = 1
        exit
      }
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "$FLOORS_FILE"
}

parse_passed_count() {
  local output_file=$1

  awk '
    /test result:/ {
      line = $0
      if (match(line, /[0-9][0-9]* passed;/)) {
        value = substr(line, RSTART, RLENGTH)
        sub(/ passed;$/, "", value)
        total += value
        found = 1
      }
      next
    }

    /^[[:space:]]*Tests[[:space:]]/ {
      line = $0
      if (match(line, /[0-9][0-9]* passed/)) {
        value = substr(line, RSTART, RLENGTH)
        sub(/ passed$/, "", value)
        total += value
        found = 1
      }
      next
    }

    /^[[:space:]]*#?[[:space:]]*pass[[:space:]]+[0-9][0-9]*[[:space:]]*$/ {
      line = $0
      sub(/^[[:space:]]*#?[[:space:]]*pass[[:space:]]+/, "", line)
      sub(/[[:space:]]*$/, "", line)
      total += line
      found = 1
    }

    END {
      if (!found) {
        exit 1
      }
      print total
    }
  ' "$output_file"
}

assert_test_counts() {
  local suite=$1 output_file=$2 floor actual

  if [[ ! -f "$FLOORS_FILE" ]]; then
    error "missing test-count floors file: $FLOORS_FILE"
    return 1
  fi
  if [[ ! -r "$output_file" ]]; then
    error "cannot read test output file: $output_file"
    return 1
  fi

  if ! floor=$(floor_for_suite "$suite"); then
    error "$suite is not listed in $FLOORS_FILE; an unlisted suite is an unmeasured suite, not an exempt one"
    return 1
  fi
  if [[ ! "$floor" =~ ^[0-9]+$ ]]; then
    error "$FLOORS_FILE has a non-numeric floor for $suite: $floor"
    return 1
  fi

  if ! actual=$(parse_passed_count "$output_file"); then
    error "could not parse a passing test count from $output_file; this suite has not been measured"
    return 1
  fi

  if (( actual < floor )); then
    error "$suite passed $actual tests, below floor $floor; a drop means tests stopped being compiled or collected - check feature flags first"
    return 1
  fi

  if (( actual >= floor + 10 )); then
    echo "::notice::$suite passed $actual tests, floor is $floor; raise $FLOORS_FILE if this increase is intentional" >&2
  fi
}

write_fixture() {
  local path=$1 content=$2

  printf '%s\n' "$content" > "$path"
}

expect_status() {
  local label=$1 expected=$2 suite=$3 fixture=$4 status=0

  assert_test_counts "$suite" "$fixture" >/dev/null 2>&1 || status=$?
  if [[ "$status" -eq "$expected" ]]; then
    self_test_passed=$((self_test_passed + 1))
  else
    printf 'self-test failed: %s expected exit %s, got %s\n' "$label" "$expected" "$status" >&2
    self_test_failed=$((self_test_failed + 1))
  fi
}

floor_for() {
  # Single source of truth for a floor, so fixtures and the real check agree.
  grep -E "^$1=" "$FLOORS_FILE" | head -1 | cut -d= -f2 | tr -d '[:space:]' | cut -d'#' -f1
}

run_self_test() {
  self_test_passed=0
  self_test_failed=0

  SELF_TEST_TEMP_DIR=$(mktemp -d)
  trap 'rm -rf -- "$SELF_TEST_TEMP_DIR"' EXIT

  # Fixtures are DERIVED FROM THE FLOORS FILE, never hard-coded. A fixture
  # tuned to a specific floor silently stops testing anything the moment that
  # floor moves: the `vitest below floor` case was written as 378 against a
  # floor of 379, and when the floor gained headroom at 360 the fixture became
  # an above-floor case that could no longer fail. Computing at/below from the
  # live floor means these cases cannot rot.
  local f_core f_ui f_node
  f_core="$(floor_for osl-hub-core)"
  f_ui="$(floor_for osl-hub-ui)"
  f_node="$(floor_for keyserver-legacy)"

  write_fixture "$SELF_TEST_TEMP_DIR/cargo-at-floor.txt" \
"test result: ok. $f_core passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
  write_fixture "$SELF_TEST_TEMP_DIR/cargo-above-floor.txt" \
"test result: ok. $((f_core + 10)) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
  write_fixture "$SELF_TEST_TEMP_DIR/vitest-at-floor.txt" \
"Tests  $f_ui passed ($f_ui)"
  write_fixture "$SELF_TEST_TEMP_DIR/node-at-floor.txt" \
"# pass $f_node"
  write_fixture "$SELF_TEST_TEMP_DIR/cargo-multiple.txt" \
"test result: ok. 100 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. $((f_core - 100)) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
  write_fixture "$SELF_TEST_TEMP_DIR/cargo-below-floor.txt" \
"test result: ok. $((f_core - 1)) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
  write_fixture "$SELF_TEST_TEMP_DIR/vitest-below-floor.txt" \
"Tests  1 failed | $((f_ui - 1)) passed ($f_ui)"
  write_fixture "$SELF_TEST_TEMP_DIR/no-count.txt" \
'all done, nothing to see here'
  : > "$SELF_TEST_TEMP_DIR/empty.txt"
  write_fixture "$SELF_TEST_TEMP_DIR/cargo-failed-parses.txt" \
'test result: FAILED. 237 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'

  expect_status "cargo at floor" 0 "osl-hub-core" "$SELF_TEST_TEMP_DIR/cargo-at-floor.txt"
  expect_status "cargo above floor" 0 "osl-hub-core" "$SELF_TEST_TEMP_DIR/cargo-above-floor.txt"
  expect_status "vitest at floor" 0 "osl-hub-ui" "$SELF_TEST_TEMP_DIR/vitest-at-floor.txt"
  expect_status "node at floor" 0 "keyserver-legacy" "$SELF_TEST_TEMP_DIR/node-at-floor.txt"
  expect_status "multiple cargo sum" 0 "osl-hub-core" "$SELF_TEST_TEMP_DIR/cargo-multiple.txt"
  expect_status "cargo below floor" 1 "osl-hub-core" "$SELF_TEST_TEMP_DIR/cargo-below-floor.txt"
  expect_status "vitest below floor" 1 "osl-hub-ui" "$SELF_TEST_TEMP_DIR/vitest-below-floor.txt"
  expect_status "no parseable count" 1 "osl-hub-core" "$SELF_TEST_TEMP_DIR/no-count.txt"
  expect_status "empty output" 1 "osl-hub-core" "$SELF_TEST_TEMP_DIR/empty.txt"
  expect_status "unlisted suite" 1 "missing-suite" "$SELF_TEST_TEMP_DIR/cargo-at-floor.txt"
  expect_status "failed cargo still parses passed count" 0 "osl-hub-core" "$SELF_TEST_TEMP_DIR/cargo-failed-parses.txt"

  printf '%s passed, %s failed\n' "$self_test_passed" "$self_test_failed"
  [[ "$self_test_failed" -eq 0 ]]
}

if [[ "${1:-}" == "--self-test" ]]; then
  [[ "$#" -eq 1 ]] || {
    usage
    exit 2
  }
  run_self_test
  exit
fi

[[ "$#" -eq 2 ]] || {
  usage
  exit 2
}

assert_test_counts "$1" "$2"
