import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const contractPath = new URL("./lifecycle.md", import.meta.url);

async function deadmanContract() {
  const document = await readFile(contractPath, "utf8");
  const match = document.match(/```deadman-contract\n([\s\S]*?)\n```/);
  assert.ok(match, "lifecycle contract must expose a deadman contract surface");
  return JSON.parse(match[1]);
}

test("dead-man contract binds removal to one stable volume and defaults to lock", async () => {
  const contract = await deadmanContract();

  assert.equal(contract.binding.scope, "one configured removable volume per setting");
  assert.equal(contract.binding.identity, "runtime-provided stable volume identifier");
  assert.equal(contract.binding.drive_letter_is_identity, false);
  assert.equal(contract.trigger.event, "bound_volume_removal_completion");
  assert.equal(contract.trigger.only_the_bound_volume, true);
  assert.equal(contract.trigger.arrival_does_not_trigger, true);
  assert.equal(contract.actions.default, "lock");
  assert.equal(contract.actions.lock.recoverable, true);
  assert.equal(contract.actions.lock.operation, "existing_session_lock");
});

test("dead-man contract makes wipe explicit and prevents locked removal escalation", async () => {
  const contract = await deadmanContract();

  assert.equal(contract.actions.wipe.recoverable, false);
  assert.equal(contract.actions.wipe.operation, "existing_burn_cleanup");
  assert.equal(
    contract.actions.wipe.enablement,
    "explicit_per_device_choice_with_exact_typed_confirmation",
  );
  assert.equal(contract.locked_removal.action, "remain_locked");
  assert.equal(contract.locked_removal.escalates_to_wipe, false);
  assert.equal(contract.locked_removal.reinsertion_unlocks, false);
});

test("dead-man contract preserves the Windows and forensic limits", async () => {
  const contract = await deadmanContract();

  assert.equal(contract.trigger.removal_while_suspended_or_hibernated_is_detectable, false);
  assert.equal(contract.limits.wipes_only_osl_controlled_data, true);
  assert.deepEqual(contract.limits.cannot_guarantee_erasure_of, [
    "Windows_swapped_data",
    "Windows_hibernated_data",
    "Windows_cached_data",
  ]);
  assert.equal(contract.limits.physical_seizure_protection, "strong_not_forensic_guarantee");
});
