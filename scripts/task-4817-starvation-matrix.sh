#!/usr/bin/env bash
# TASK 4817 starvation matrix.
#
# Every leg of the confidentiality proof is removed one at a time. Each run must
# exit 1 and name the missing proof; a case that exits 0 means the check would
# pass with that leg absent, which makes the check decoration.
set -u

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

CASES=(
  "allowed_kind:identity_public_profile"
  "allowed_kind:friend_roster"
  "allowed_kind:peer_public_key_bundles"
  "allowed_kind:safety_number_pins"
  "allowed_kind:conversation_membership"
  "allowed_kind:server_whitelist_rules"
  "allowed_kind:channel_whitelist_rules"
  "allowed_kind:message_metadata"
  "allowed_kind:attachment_pointers"
  "allowed_kind:post_pairing_messages"
  "receiver_readback"
  "relay_surface:process_memory"
  "relay_surface:request_bodies"
  "relay_surface:response_bodies"
  "relay_surface:queues"
  "relay_surface:databases"
  "relay_surface:blobs"
  "relay_surface:caches"
  "relay_surface:logs"
  "relay_surface:telemetry"
  "relay_surface:crash_exports"
  "relay_surface:packet_capture"
  "wrong_key"
  "ordinary_control"
  "plaintext_mutation"
  "tls_only"
  "envelope_shape_only"
  "refuse_sync"
)

red=0
green=0
for case in "${CASES[@]}"; do
  out=$(TASK4817_STARVE="$case" cargo test -p ipc \
    --test task_4817_sealed_sync_confidentiality -- --nocapture --test-threads=1 2>&1)
  code=$?
  reason=$(printf '%s\n' "$out" | grep -m1 '^TASK4817_MISSING ' | cut -c18-)
  if [ "$code" -eq 0 ]; then
    green=$((green + 1))
    echo "TASK4817_STARVE_CASE case=$case exit=$code STILL_GREEN"
  else
    red=$((red + 1))
    echo "TASK4817_STARVE_CASE case=$case exit=$code missing=\"$reason\""
  fi
done

echo "TASK4817_STARVE_TOTAL cases=${#CASES[@]} exited_1=$red exited_0=$green"
[ "$green" -eq 0 ] || exit 1
