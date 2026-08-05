#!/usr/bin/env bash
# Mutation harness for the three OSL Chat wire-ins.
#
# Each mutant unwires ONE thing and runs the gate that is supposed to notice.
# A gate that stays green under its own mutant is decoration, not a gate.
#
# Usage: ./mutants.sh [name ...]   (default: all)
set -u
cd "$(dirname "$0")"
export CARGO_TARGET_DIR="$PWD/.target-chatwire"

BROKER=apps/osl-hub/src/broker.rs
BACKEND=apps/osl-hub/src/secure_disk_backend.rs

revert() { git checkout -- "$BROKER" "$BACKEND" 2>/dev/null; }
trap revert EXIT

run_gate() { # $1 = cargo test args...
  flock -o /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    "$@" -j 3 -- --test-threads=1 >/tmp/mutant-run.log 2>&1
  echo $?
}

report() { printf '%-28s %-46s exit=%s  %s\n' "$1" "$2" "$3" "$4"; }

mutant_enqueue() {
  revert
  # Unwire the D-223 enqueue: never persist the undelivered relay notice.
  perl -0pi -e 's/if crate::osl_chat_queue::is_unreachable\(&_error\)/if false \&\& crate::osl_chat_queue::is_unreachable(&_error)/' "$BROKER"
  grep -q 'if false && crate::osl_chat_queue::is_unreachable' "$BROKER" || { echo "MUTANT enqueue DID NOT APPLY"; return; }
  code=$(run_gate --features core --test osl_chat_queued_send)
  report "M1 enqueue-unwired" "tests/osl_chat_queued_send.rs" "$code" "$([ "$code" != 0 ] && echo 'RED (gate works)' || echo '*** GREEN = DECORATION ***')"
  revert
}

mutant_drain() {
  revert
  # Unwire the reconnect drain from the OSL Chat receive path.
  perl -0pi -e 's/    let _ = drain_osl_chat_send_queue\(core\);/    \/\/ mutant: drain removed/' "$BROKER"
  grep -q '// mutant: drain removed' "$BROKER" || { echo "MUTANT drain DID NOT APPLY"; return; }
  code=$(run_gate --features core --test osl_chat_queued_send)
  report "M2 drain-unwired" "tests/osl_chat_queued_send.rs" "$code" "$([ "$code" != 0 ] && echo 'RED (gate works)' || echo '*** GREEN = DECORATION ***')"
  revert
}

mutant_backend() {
  revert
  # Break durability: the store forgets everything it wrote.
  perl -0pi -e 's/    fn read_blob\(&self, storage_key: &str\) -> Result<Option<Vec<u8>>, SecureLocalStoreError> \{/    fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {\n        if true { let _ = storage_key; return Ok(None); }/' "$BACKEND"
  grep -q 'if true { let _ = storage_key; return Ok(None); }' "$BACKEND" || { echo "MUTANT backend DID NOT APPLY"; return; }
  code=$(run_gate --lib secure_disk_backend)
  report "M3 backend-not-durable" "src/secure_disk_backend.rs lib tests" "$code" "$([ "$code" != 0 ] && echo 'RED (gate works)' || echo '*** GREEN = DECORATION ***')"
  revert
}

mutant_scope() {
  revert
  # Unwire the relay scope id on the receive side: accept any scope.
  perl -0pi -e 's/        if item\.sender_id != manual\.peer_osl_user_id \|\| item\.scope_id != scope_id \{\n            return None;\n        \}\n        let Ok\(bundle\) = STANDARD\.decode\(&item\.bundle_b64\) else \{\n            return None;\n        \};\n        if !ipc::wire_v2::is_native_overlay_relay_bundle/        if item.sender_id != manual.peer_osl_user_id {\n            return None;\n        }\n        let Ok(bundle) = STANDARD.decode(&item.bundle_b64) else {\n            return None;\n        };\n        if !ipc::wire_v2::is_native_overlay_relay_bundle/' "$BROKER"
  code=$(run_gate --features core --test osl_chat_lost_response)
  report "M4 scope-id-unwired" "tests/osl_chat_lost_response.rs" "$code" "$([ "$code" != 0 ] && echo 'RED (gate works)' || echo '*** GREEN = DECORATION ***')"
  revert
}

for name in "${@:-enqueue drain backend scope}"; do
  case "$name" in
    enqueue) mutant_enqueue ;;
    drain)   mutant_drain ;;
    backend) mutant_backend ;;
    scope)   mutant_scope ;;
    *) echo "unknown mutant: $name" ;;
  esac
done
