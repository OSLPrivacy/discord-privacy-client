#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
guide="${OSL_TWO_COPY_GUIDE_DOC:-${repo_root}/docs/testing/azure-vm-qa-workflow.md}"

fail() {
  printf 'test-osl-two-copy-guide: %s\n' "$1" >&2
  exit 1
}

extract_command() {
  local source="$1"
  awk '
    /# OSL-TWO-COPY-FAST-STATUS-BEGIN/ { found = 1 }
    found { print }
    /# OSL-TWO-COPY-FAST-STATUS-END/ { exit }
  ' "$source"
}

assert_required_status_command() {
  local command="$1" copy="$2" data_var="$3" identity_var="$4"
  local compact
  compact="$(printf '%s\n' "$command" | tr -d '\\' | tr '\n' ' ')"
  if [[ ! "$compact" =~ scripts/qa/osl-two-copy-process-status\.sh[[:space:]]+--copy[[:space:]]+${copy}[[:space:]]+--data[[:space:]]+\"\$${data_var}\"[[:space:]]+--identity-name[[:space:]]+\"\$${identity_var}\" ]]; then
    fail "guide is missing the exact process-status command for copy ${copy}"
  fi
}

run_guide_check() {
  local command="$1"
  local tmp out pid_a pid_b
  tmp="$(mktemp -d)"
  out="${tmp}/out.txt"
  trap 'rm -rf "$tmp"' RETURN

  [[ -n "$command" ]] || fail "missing OSL-TWO-COPY-FAST-STATUS block"
  [[ "$(grep -c 'scripts/qa/osl-two-copy-startup.sh' <<<"$command")" -eq 1 ]] ||
    fail "guide must contain exactly one two-copy startup command"
  [[ "$(grep -c 'scripts/qa/osl-two-copy-process-status.sh' <<<"$command")" -eq 2 ]] ||
    fail "guide must contain exactly two process-status commands"
  assert_required_status_command "$command" A data_a identity_a
  assert_required_status_command "$command" B data_b identity_b

  printf '%s\n' "$command" >"${tmp}/guide-command.sh"
  (
    cd "$repo_root"
    bash "${tmp}/guide-command.sh"
  ) >"$out"
  cat "$out"

  [[ "$(grep -c '^TASK0031 two-copy-process status copy=' "$out")" -eq 2 ]] ||
    fail "guide command did not print exactly two live process status results"
  grep -q '^TASK0031 two-copy-process status copy=A ' "$out" ||
    fail "guide command did not print live status for copy A"
  grep -q '^TASK0031 two-copy-process status copy=B ' "$out" ||
    fail "guide command did not print live status for copy B"

  pid_a="$(sed -n 's/^TASK0031 two-copy-process status copy=A pid=\([0-9][0-9]*\).*/\1/p' "$out")"
  pid_b="$(sed -n 's/^TASK0031 two-copy-process status copy=B pid=\([0-9][0-9]*\).*/\1/p' "$out")"
  [[ -n "$pid_a" && -n "$pid_b" && "$pid_a" != "$pid_b" ]] ||
    fail "guide command did not produce two distinct live process numbers"

  printf 'TASK0033 two-copy-guide live_status_count=2\n'
  printf 'TASK0033 two-copy-guide live_status_copies=A,B\n'
  printf 'TASK0033 two-copy-guide live_status_pids=%s,%s\n' "$pid_a" "$pid_b"
}

remove_status_block() {
  local copy="$1" source="$2" destination="$3"
  awk -v copy="--copy ${copy}" '
    skip == 0 && /scripts\/qa\/osl-two-copy-process-status\.sh[[:space:]]*\\/ {
      buffer = $0 ORS
      skip = 1
      next
    }
    skip == 1 {
      buffer = buffer $0 ORS
      if ($0 ~ /--identity-name/) {
        if (buffer ~ copy) {
          buffer = ""
          skip = 0
          next
        }
        printf "%s", buffer
        buffer = ""
        skip = 0
      }
      next
    }
    { print }
    END {
      if (skip == 1 && buffer != "") {
        printf "%s", buffer
      }
    }
  ' "$source" >"$destination"
}

expect_removed_copy_fails() {
  local copy="$1" tmp mutant rc
  tmp="$(mktemp -d)"
  mutant="${tmp}/guide-without-copy-${copy}.md"
  remove_status_block "$copy" "$guide" "$mutant"
  set +e
  OSL_TWO_COPY_GUIDE_POSITIVE_ONLY=1 \
    OSL_TWO_COPY_GUIDE_DOC="$mutant" \
    "$0" >"${tmp}/out.txt" 2>"${tmp}/err.txt"
  rc=$?
  set -e
  cat "${tmp}/out.txt"
  cat "${tmp}/err.txt" >&2
  rm -rf "$tmp"
  [[ "$rc" -ne 0 ]] || fail "removing copy ${copy} status command did not fail the guide check"
  printf 'TASK0033 two-copy-guide removed_copy=%s check_exit=%s\n' "$copy" "$rc"
}

command="$(extract_command "$guide")"
run_guide_check "$command"

if [[ "${OSL_TWO_COPY_GUIDE_POSITIVE_ONLY:-0}" != "1" ]]; then
  expect_removed_copy_fails A
  expect_removed_copy_fails B
fi
