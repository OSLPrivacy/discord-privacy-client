import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const contractPath = new URL("./ratchet.md", import.meta.url);

async function contractSurface() {
  const document = await readFile(contractPath, "utf8");
  const match = document.match(/```ratchet-contract\n([\s\S]*?)\n```/);
  assert.ok(match, "ratchet contract must expose a machine-readable surface");
  return JSON.parse(match[1]);
}

test("ratchet contract freezes D41's durable no-silent-recovery surface", async () => {
  const surface = await contractSurface();

  assert.deepEqual(surface.health.states, ["Healthy", "Degraded", "Desynced", "Unrecoverable"]);
  assert.equal(surface.health.auth_failures_to_desynced, 3);
  assert.deepEqual(surface.health.desync_triggers, [
    "three_consecutive_auth_failed_from_pinned_peer",
    "no_session_on_file_for_pinned_peer",
    "skip_bound_refusal",
  ]);
  assert.equal(surface.health.durable, true);
  assert.equal(surface.health.only_successful_decrypt_clears_desynced, true);
  assert.equal(surface.recovery.reset_wire, "v3");
  assert.equal(surface.recovery.throttle_arms, "confirmed_delivery");
  assert.equal(surface.recovery.session_delete_keeps_pin, true);
  assert.equal(surface.recovery.bootstrap_ping_heals_one_directional_desync, true);
});

test("ratchet contract forbids downgrade and nonce-burning retries", async () => {
  const surface = await contractSurface();

  assert.equal(surface.selection.live_capability, "RN_CAP_WIRE_RN_LIVE");
  assert.equal(surface.selection.live_capability_bit, 1);
  assert.equal(surface.selection.unpinned_requires_verified_live_capability_for_rn, true);
  assert.equal(surface.selection.pinned_never_selects_v3, true);
  assert.equal(surface.selection.pin_lowering, "explicit_both_sides_confirmed_out_of_band_unpin_only");
  assert.equal(surface.storage.exported_session_directory_constant, "RN_SESSION_DIR");
  assert.equal(surface.storage.one_writer_per_peer, true);
  assert.equal(surface.storage.lock_loser, "named_error_no_stale_retry");
  assert.equal(surface.outbox.encrypt_once, "at_enqueue");
  assert.equal(surface.outbox.retry, "reuse_stored_rn_wire");
  assert.equal(surface.outbox.version_fallback, "forbidden");
  assert.equal(Object.keys(surface.error_mapping).length, 11);
  assert.ok(Object.values(surface.error_mapping).every((state) => state.length > 0));
});

