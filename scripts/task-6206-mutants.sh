#!/usr/bin/env bash
# TASK 6206 — the production-mutant ladder.
#
# Every deployed content-write endpoint applies four guards: membership, role,
# author and parent binding. This bypasses each of those twenty guards, one at a
# time, in its own separate throwaway deployed build, and requires THE SAME
# unchanged check to exit 1 naming four things on one line: the endpoint, the
# hostile identity, the binding that request forged, and the forbidden deployed
# state it reached. A twenty-first rung does the same to the pre-existing
# self-role guard the rights-write endpoint uses.
#
# Signed requests, identifiers, expectations and the checker never move: the
# script fingerprints the checker, the installed client and the deployed service
# transport before every rung and refuses to continue if any of them changed. It
# also reads the production guard inventory out of the source and fails if any
# guard has no rung — one mutant cannot stand in for an endpoint nothing
# bypassed.
#
#   scripts/task-6206-mutants.sh
#
# TASK6206_SKIP_MUTANT=<name>  starves one rung, to show the ladder notices.
# TASK6206_SKIP_RESTORED=1     starves the restored run.
# TASK6206_LADDER_DRY=1        walks the bookkeeping without building.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set to the target directory for this lane}"

GUARDS="crates/ipc/src/chats_content_guards.rs"
AUTHORITY="crates/ipc/src/chats_service_authority.rs"
CHECKER="crates/ipc/tests/task_6206_deployed_chats_content_writes.rs"
CLIENT="crates/ipc/src/bin/osl-chats-client.rs"
SERVICE="crates/ipc/src/bin/osl-chats-service.rs"
MANIFEST="crates/ipc/src/chats_route_manifest.rs"
SKIP="${TASK6206_SKIP_MUTANT:-}"
DRY="${TASK6206_LADDER_DRY:-}"
SKIP_RESTORED="${TASK6206_SKIP_RESTORED:-}"

# name | file | marker | endpoint token | hostile identity token | forged binding token | forbidden state token
MUTANTS=(
"message.create-membership|$GUARDS|TASK6206-GUARD-MESSAGE-CREATE-MEMBERSHIP|endpoint=message.create |identity=\"Del 6206\"|channel=channel-6206-open-11a0|message:message-6206-hostile-excluded-create"
"message.create-role|$GUARDS|TASK6206-GUARD-MESSAGE-CREATE-ROLE|endpoint=message.create |identity=\"Ben 6206\"|channel=channel-6206-open-11a0|message:message-6206-hostile-member-create"
"message.create-author|$GUARDS|TASK6206-GUARD-MESSAGE-CREATE-AUTHOR|endpoint=message.create |identity=\"Cy 6206\"|author=Ada 6206|message:message-6206-hostile-forged-author"
"message.create-parent|$GUARDS|TASK6206-GUARD-MESSAGE-CREATE-PARENT|endpoint=message.create |identity=\"Cy 6206\"|thread=thread-6206-child-42d0|message:message-6206-hostile-thread-parent"
"message.edit-membership|$GUARDS|TASK6206-GUARD-MESSAGE-EDIT-MEMBERSHIP|endpoint=message.edit |identity=\"Del 6206\"|message=message-6206-open-del-55d0|message:message-6206-open-del-55d0"
"message.edit-role|$GUARDS|TASK6206-GUARD-MESSAGE-EDIT-ROLE|endpoint=message.edit |identity=\"Ben 6206\"|message=message-6206-open-ben-own-53b1|message:message-6206-open-ben-own-53b1"
"message.edit-author|$GUARDS|TASK6206-GUARD-MESSAGE-EDIT-AUTHOR|endpoint=message.edit |identity=\"Ada 6206\"|message=message-6206-open-ben-keep-52b0|message:message-6206-open-ben-keep-52b0"
"message.edit-parent|$GUARDS|TASK6206-GUARD-MESSAGE-EDIT-PARENT|endpoint=message.edit |identity=\"Cy 6206\"|channel=channel-6206-open-11a0|message:message-6206-limited-keep-59c2"
"message.delete-membership|$GUARDS|TASK6206-GUARD-MESSAGE-DELETE-MEMBERSHIP|endpoint=message.delete |identity=\"Del 6206\"|message=message-6206-open-del-55d0|message:message-6206-open-del-55d0"
"message.delete-role|$GUARDS|TASK6206-GUARD-MESSAGE-DELETE-ROLE|endpoint=message.delete |identity=\"Ben 6206\"|message=message-6206-open-ben-own-53b1|message:message-6206-open-ben-own-53b1"
"message.delete-author|$GUARDS|TASK6206-GUARD-MESSAGE-DELETE-AUTHOR|endpoint=message.delete |identity=\"Cy 6206\"|message=message-6206-open-ben-keep-52b0|message:message-6206-open-ben-keep-52b0"
"message.delete-parent|$GUARDS|TASK6206-GUARD-MESSAGE-DELETE-PARENT|endpoint=message.delete |identity=\"Ada 6206\"|enclave=enclave-6206-other-d520|message:message-6206-open-ben-keep-52b0"
"channel.create-membership|$GUARDS|TASK6206-GUARD-CHANNEL-CREATE-MEMBERSHIP|endpoint=channel.create |identity=\"Del 6206\"|channel=channel-6206-hostile-excluded|channel:channel-6206-hostile-excluded"
"channel.create-role|$GUARDS|TASK6206-GUARD-CHANNEL-CREATE-ROLE|endpoint=channel.create |identity=\"Ben 6206\"|channel=channel-6206-hostile-member|channel:channel-6206-hostile-member"
"channel.create-author|$GUARDS|TASK6206-GUARD-CHANNEL-CREATE-AUTHOR|endpoint=channel.create |identity=\"Cy 6206\"|creator=Ada 6206|channel:channel-6206-hostile-forged-creator"
"channel.create-parent|$GUARDS|TASK6206-GUARD-CHANNEL-CREATE-PARENT|endpoint=channel.create |identity=\"Cy 6206\"|enclave=enclave-6206-other-d520|channel:channel-6206-hostile-foreign-enclave"
"thread.create-membership|$GUARDS|TASK6206-GUARD-THREAD-CREATE-MEMBERSHIP|endpoint=thread.create |identity=\"Del 6206\"|channel=channel-6206-open-11a0|thread:thread-6206-hostile-excluded"
"thread.create-role|$GUARDS|TASK6206-GUARD-THREAD-CREATE-ROLE|endpoint=thread.create |identity=\"Ben 6206\"|channel=channel-6206-open-11a0|thread:thread-6206-hostile-member"
"thread.create-author|$GUARDS|TASK6206-GUARD-THREAD-CREATE-AUTHOR|endpoint=thread.create |identity=\"Cy 6206\"|creator=Ada 6206|thread:thread-6206-hostile-forged-creator"
"thread.create-parent|$GUARDS|TASK6206-GUARD-THREAD-CREATE-PARENT|endpoint=thread.create |identity=\"Cy 6206\"|enclave=enclave-6206-other-d520|thread:thread-6206-hostile-foreign-enclave"
"role.grant-self-role|$AUTHORITY|TASK6120-GUARD-SELF-ROLE|endpoint=role.grant |identity=\"Ben 6206\"|target=Ben 6206|rights:Ben 6206"
)

fingerprint() {
  sha256sum "$CHECKER" "$CLIENT" "$SERVICE" "$MANIFEST" | awk '{print $1}' | tr '\n' ' '
}
FROZEN_HARNESS="$(fingerprint)"
echo "TASK6206MUTANT harness_fingerprint=$FROZEN_HARNESS"

# Every production guard in the deployed inventory has to have a rung. A guard
# nobody bypasses is a guard nobody proved.
declare -a INVENTORY_MARKERS
while IFS= read -r marker; do
  INVENTORY_MARKERS+=("$marker")
done < <(grep -o 'TASK6206-GUARD-[A-Z-]*' "$GUARDS" | sort -u)
echo "TASK6206MUTANT production_guards=${#INVENTORY_MARKERS[@]}"
missing_rung=0
for marker in "${INVENTORY_MARKERS[@]}"; do
  found=0
  for spec in "${MUTANTS[@]}"; do
    case "$spec" in *"|$marker|"*) found=1 ;; esac
  done
  if [ "$found" -eq 0 ]; then
    echo "TASK6206MUTANT FAIL the production guard $marker has no mutant rung"
    missing_rung=1
  fi
done

apply_guard() {
  python3 - "$1" "$2" <<'PY'
import sys
path, marker = sys.argv[1], sys.argv[2]
lines = open(path).read().splitlines(keepends=True)
needle = f"// {marker}"
hits = [i for i, line in enumerate(lines) if needle in line]
if len(hits) != 1:
    print(f"MUTATE-ERROR expected one guard body for {marker}, found {len(hits)}")
    sys.exit(2)
lines[hits[0]] = f"    true // {marker} (BYPASSED)\n"
open(path, "w").write("".join(lines))
print(f"MUTATE-OK {marker}")
PY
}

ran=()
failed=$missing_rung

for spec in "${MUTANTS[@]}"; do
  IFS='|' read -r name file marker endpoint identity forged state <<<"$spec"
  if [ "$name" = "$SKIP" ]; then
    echo "TASK6206MUTANT skipped=$name"
    continue
  fi
  echo "=== TASK6206MUTANT $name ($marker in $file) ==="
  if [ -n "$DRY" ]; then
    echo "TASK6206MUTANT $name accounted (dry)"
    ran+=("$name")
    continue
  fi
  cp "$file" "$file.task6206.bak"
  if ! apply_guard "$file" "$marker"; then
    echo "TASK6206MUTANT FAIL $name could not be applied"
    failed=1
  fi
  if cmp -s "$file" "$file.task6206.bak"; then
    echo "TASK6206MUTANT FAIL $name did not change $file"
    failed=1
  fi
  if [ "$(fingerprint)" != "$FROZEN_HARNESS" ]; then
    echo "TASK6206MUTANT FAIL $name changed the signed requests, identifiers, expectations or checker"
    failed=1
  fi

  out="$(bash scripts/task-6206-check.sh "mutant-$name" 2>&1)"
  code=$?
  echo "$out" | grep -E '^TASK6206 (SUMMARY|RESULT)' | head -3
  leak="$(echo "$out" \
    | grep '^TASK6206 FAIL hostile write reached deployed state' \
    | grep -F "$endpoint" | grep -F "$identity" | grep -F "$forged" | grep -F "$state" \
    | head -1)"
  if [ "$code" -ne 1 ]; then
    echo "TASK6206MUTANT FAIL $name: the check exited $code, expected 1"
    failed=1
  elif [ -z "$leak" ]; then
    echo "TASK6206MUTANT FAIL $name: the check exited 1 without one line naming endpoint/identity/forged binding/forbidden state"
    echo "$out" | grep '^TASK6206 FAIL' | head -4
    failed=1
  else
    echo "TASK6206MUTANT $name exit=1"
    echo "  named: $leak"
  fi
  mv "$file.task6206.bak" "$file"
  # cp/mv carry the backup's older timestamp forward and cargo decides whether
  # to rebuild by mtime: without this touch the next build silently reuses the
  # mutant binary.
  touch "$file"
  ran+=("$name")
done

echo "=== TASK6206MUTANT restored ==="
if [ -f "$GUARDS.task6206.bak" ] || [ -f "$AUTHORITY.task6206.bak" ]; then
  echo "TASK6206MUTANT FAIL a mutation was left behind"
  failed=1
fi
if [ "$(fingerprint)" != "$FROZEN_HARNESS" ]; then
  echo "TASK6206MUTANT FAIL the checker or installed client changed across the ladder"
  failed=1
fi
if [ -n "$DRY" ]; then
  out="TASK6206 RESULT pass (dry)"
  if [ -n "$SKIP_RESTORED" ]; then code=1; else code=0; fi
else
  out="$(bash scripts/task-6206-check.sh restored 2>&1)"
  code=$?
fi
echo "$out" | grep -E '^TASK6206 (SUMMARY|RESULT|CHECK)|^TASK6206 FAIL' | head -8
if [ "$code" -ne 0 ]; then
  echo "TASK6206MUTANT FAIL the restored build did not pass"
  failed=1
else
  ran+=("restored")
  echo "TASK6206MUTANT restored exit=0"
fi

for spec in "${MUTANTS[@]}"; do
  IFS='|' read -r name _ <<<"$spec"
  found=0
  for done_name in "${ran[@]}"; do
    [ "$done_name" = "$name" ] && found=1
  done
  if [ "$found" -eq 0 ]; then
    echo "TASK6206MUTANT FAIL the production guard mutant $name never ran"
    failed=1
  fi
done
case " ${ran[*]} " in
  *" restored "*) ;;
  *) echo "TASK6206MUTANT FAIL the restored run never ran"; failed=1 ;;
esac

echo "TASK6206MUTANT LADDER mutants=${#MUTANTS[@]} ran=${#ran[@]} rungs=${ran[*]}"
if [ "$failed" -ne 0 ]; then
  echo "TASK6206MUTANT RESULT fail"
  exit 1
fi
echo "TASK6206MUTANT RESULT pass"
exit 0
