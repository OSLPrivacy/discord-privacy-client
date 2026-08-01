#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

expected_files=$(mktemp)
actual_files=$(mktemp)
trap 'rm -f "$expected_files" "$actual_files"' EXIT

cat >"$expected_files" <<'EOF'
core/a115_friend_request_decline_revoke.rs
core/a7_duress_auto_lock_anchor_acceptance.rs
core/a7_session_lock_stops_decryption.rs
core/b3_ipc_integration_proof_status.rs
core/bilateral_burn.rs
core/commands_test.rs
core/friend_request_roundtrip.rs
core/keyserver_url_default.rs
core/prose_token_live.rs
core/register_after_unlock.rs
core/register_fix_peer_keys.rs
core/register_self_snowflake_requires_account_ownership_proof.rs
core/sender_attribution_proof.rs
core/sender_attribution_regression.rs
core/threat_model_reconciliation.rs
core/whitelist_repair.rs
core/whitelist_self_guard.rs
lifecycle/phase_b1_app_preferences.rs
lifecycle/phase_b1_burn_drops_mode1.rs
lifecycle/phase_b1_decode_dispatch.rs
lifecycle/phase_b1_encrypt_output_dispatch.rs
lifecycle/phase_b3_f1_plaintext_substitution.rs
lifecycle/phase_d_fix2_reload.rs
lifecycle/phase_d_tour.rs
lifecycle/phase_f0_fix1_mkdir.rs
lifecycle/phase_f0_fix2_identity_gen.rs
lifecycle/phase_f2_2_license_cache.rs
lifecycle/phase_f2_4_license_lifecycle.rs
lifecycle/phase_f3_6_attachment_gate.rs
lifecycle/phase_g3_1_update_check.rs
lifecycle/phase_g3_3_update_channel.rs
lifecycle/phase_td1_persist_error.rs
ratchet/phase_a1_1_recovery_rekey.rs
ratchet/phase_a1_v3_wire.rs
ratchet/phase_a1b_keyserver_mlkem_populate.rs
ratchet/phase_a1c_burn_kill_list.rs
ratchet/phase_a2_integration_dr_roundtrip.rs
ratchet/phase_a2_peer_map_ratchet.rs
ratchet/phase_a2_wire_v4.rs
ratchet/phase_a3_integration_sk_roundtrip.rs
ratchet/phase_a3_prekey_startup.rs
ratchet/phase_a3_sender_key_state_file.rs
ratchet/phase_a3_skdm.rs
ratchet/phase_a3_wire_v5.rs
storage/at_rest_boundary_acceptance.rs
storage/peer_map_decrypt.rs
storage/phase_perf1_peer_map_encryption.rs
sync/phase_c1_bulk.rs
sync/phase_c1_handshake_removal.rs
sync/phase_c1_migration.rs
sync/phase_c1_summary.rs
sync/phase_c2_bulk_dm.rs
sync/phase_c3_server_defaults.rs
wire/osl_phase4_roundtrip.rs
wire/osl_phase5_decrypt.rs
wire/phase5_production_flow.rs
wire/phase5b2_persistence.rs
wire/phase6a_edit.rs
wire/phase7a_fresh_start.rs
wire/phase7a_wire_v2.rs
wire/phase7b_control_messages.rs
wire/phase7b_send_recv_integration.rs
wire/phase7b_whitelist.rs
wire/phase7d_fix3_self_entry.rs
wire/phase8_attachment_integration.rs
EOF

find core wire ratchet lifecycle sync storage -maxdepth 1 -type f -name '*.rs' ! -name mod.rs -printf '%p\n' | sed 's#^\./##' | sort >"$actual_files"
diff -u "$expected_files" "$actual_files"

target_count=$(find . -maxdepth 1 -type f -name '*.rs' | wc -l | tr -d ' ')
test "$target_count" = 6
