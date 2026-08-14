#!/usr/bin/env bash
# TASK 6120 — the production-mutant ladder.
#
# For each named production check, this bypasses that check in the source, runs
# the SAME check against a separate throwaway deployed build, and requires it to
# exit 1 naming the identifier, ciphertext or raised right that leaked. It then
# restores the source and requires a green run.
#
# Client labels, fixtures, expected values and the checker are never touched:
# the script refuses to continue if any file other than the one being mutated
# has changed, and it names every mutant or restored run that did not happen.
#
#   scripts/task-6120-mutants.sh
#
# TASK6120_SKIP_MUTANT=<name> starves one rung, to show the ladder notices.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the target directory for this lane}"

AUTHORITY="crates/ipc/src/chats_service_authority.rs"
MEMBERSHIP="crates/ipc/src/server_membership.rs"
CHECKER="crates/ipc/tests/task_6120_deployed_chats_authorization.rs"
CLIENT="crates/ipc/src/bin/osl-chats-client.rs"
SERVICE="crates/ipc/src/bin/osl-chats-service.rs"
SKIP="${TASK6120_SKIP_MUTANT:-}"
# Accounting-only mode: walk the ladder's bookkeeping without building or
# deploying anything, so that starving a rung is demonstrable in seconds.
DRY="${TASK6120_LADDER_DRY:-}"
SKIP_RESTORED="${TASK6120_SKIP_RESTORED:-}"

# name | file | marker or exact source line | replacement | token the failure must name
MUTANTS=(
"self-role|$AUTHORITY|GUARD|TASK6120-GUARD-SELF-ROLE|remove-people"
"roster-channel-list|$AUTHORITY|GUARD|TASK6120-GUARD-DIRECTORY|Ada TASK6120"
"restricted-history|$AUTHORITY|GUARD|TASK6120-GUARD-RESTRICTED-HISTORY|channel-6120-limited-20b0"
"child-thread-inheritance|$AUTHORITY|GUARD|TASK6120-GUARD-THREAD-INHERITANCE|thread-6120-child-30c0"
"known-id-fetch|$AUTHORITY|GUARD|TASK6120-GUARD-KNOWN-ID-FETCH|message-6120-restricted-50e0"
"upstream-thread-readers|$MEMBERSHIP|EXACT|Ok(channel.effective_readers(members))|thread-6120-child-30c0"
)

fingerprint() {
  sha256sum "$CHECKER" "$CLIENT" "$SERVICE" | awk '{print $1}' | tr '\n' ' '
}
FROZEN_HARNESS="$(fingerprint)"
echo "TASK6120MUTANT harness_fingerprint=$FROZEN_HARNESS"

apply_guard() {
  python3 - "$1" "$2" <<'PY'
import sys
path, marker = sys.argv[1], sys.argv[2]
lines = open(path).read().splitlines(keepends=True)
hits = [i for i, line in enumerate(lines) if marker in line and "fn " not in line]
if len(hits) != 1:
    print(f"MUTATE-ERROR expected one guard line for {marker}, found {len(hits)}")
    sys.exit(2)
lines[hits[0]] = f"    true // {marker} (BYPASSED)\n"
open(path, "w").write("".join(lines))
print(f"MUTATE-OK {marker}")
PY
}

apply_exact() {
  python3 - "$1" "$2" <<'PY'
import sys
path, needle = sys.argv[1], sys.argv[2]
text = open(path).read()
if text.count(needle) != 1:
    print(f"MUTATE-ERROR expected one occurrence of {needle}, found {text.count(needle)}")
    sys.exit(2)
replacement = "Ok(members.members().into_iter().map(|member| member.name).collect())"
open(path, "w").write(text.replace(needle, replacement, 1))
print(f"MUTATE-OK {needle}")
PY
}

ran=()
failed=0

for spec in "${MUTANTS[@]}"; do
  IFS='|' read -r name file kind target token <<<"$spec"
  if [ "$name" = "$SKIP" ]; then
    echo "TASK6120MUTANT skipped=$name"
    continue
  fi
  echo "=== TASK6120MUTANT $name ($kind on $file) ==="
  if [ -n "$DRY" ]; then
    echo "TASK6120MUTANT $name accounted (dry)"
    ran+=("$name")
    continue
  fi
  cp "$file" "$file.task6120.bak"
  if [ "$kind" = "GUARD" ]; then
    apply_guard "$file" "$target" || { echo "TASK6120MUTANT $name could not be applied"; failed=1; }
  else
    apply_exact "$file" "$target" || { echo "TASK6120MUTANT $name could not be applied"; failed=1; }
  fi
  if cmp -s "$file" "$file.task6120.bak"; then
    echo "TASK6120MUTANT FAIL $name did not change $file"
    failed=1
  fi
  if [ "$(fingerprint)" != "$FROZEN_HARNESS" ]; then
    echo "TASK6120MUTANT FAIL $name changed the client labels, fixtures, expected values or checker"
    failed=1
  fi

  out="$(bash scripts/task-6120-check.sh "mutant-$name" 2>&1)"
  code=$?
  echo "$out" | grep -E '^TASK6120 (SUMMARY|RESULT)|^TASK6120 FAIL' | head -10
  echo "$out" | grep -E '^TASK6120 CHECK'
  if [ "$code" -ne 1 ]; then
    echo "TASK6120MUTANT FAIL $name: the check exited $code, expected 1"
    failed=1
  elif ! echo "$out" | grep -q "^TASK6120 FAIL .*$token"; then
    echo "TASK6120MUTANT FAIL $name: the check exited 1 without naming $token"
    failed=1
  else
    echo "TASK6120MUTANT $name exit=1 named=$token"
  fi
  mv "$file.task6120.bak" "$file"
  # cp/mv carry the backup's older timestamp forward, and cargo decides whether
  # to rebuild by mtime: without this touch the next build would silently reuse
  # the mutant binary.
  touch "$file"
  ran+=("$name")
done

echo "=== TASK6120MUTANT restored ==="
if [ -n "$(git status --porcelain -- "$AUTHORITY" "$MEMBERSHIP" | grep -E '^ M' || true)" ] && [ -f "$AUTHORITY.task6120.bak" ]; then
  echo "TASK6120MUTANT FAIL a mutation was left behind"
  failed=1
fi
if [ "$(fingerprint)" != "$FROZEN_HARNESS" ]; then
  echo "TASK6120MUTANT FAIL the checker or clients changed across the ladder"
  failed=1
fi
if [ -n "$DRY" ]; then
  out="TASK6120 RESULT pass (dry)"
  if [ -n "$SKIP_RESTORED" ]; then code=1; else code=0; fi
else
  out="$(bash scripts/task-6120-check.sh restored 2>&1)"
  code=$?
fi
echo "$out" | grep -E '^TASK6120 (SUMMARY|RESULT|CHECK)|^TASK6120 FAIL' | head -12
if [ "$code" -ne 0 ]; then
  echo "TASK6120MUTANT FAIL the restored build did not pass"
  failed=1
else
  ran+=("restored")
  echo "TASK6120MUTANT restored exit=0"
fi

for spec in "${MUTANTS[@]}"; do
  IFS='|' read -r name _ <<<"$spec"
  found=0
  for done_name in "${ran[@]}"; do
    [ "$done_name" = "$name" ] && found=1
  done
  if [ "$found" -eq 0 ]; then
    echo "TASK6120MUTANT FAIL the production mutant $name never ran"
    failed=1
  fi
done
case " ${ran[*]} " in
  *" restored "*) ;;
  *) echo "TASK6120MUTANT FAIL the restored run never ran"; failed=1 ;;
esac

echo "TASK6120MUTANT LADDER mutants=${#MUTANTS[@]} ran=${#ran[@]} rungs=${ran[*]}"
if [ "$failed" -ne 0 ]; then
  echo "TASK6120MUTANT RESULT fail"
  exit 1
fi
echo "TASK6120MUTANT RESULT pass"
exit 0
