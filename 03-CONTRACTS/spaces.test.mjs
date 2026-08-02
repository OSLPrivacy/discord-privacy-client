import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const contractPath = new URL("./spaces.md", import.meta.url);

async function roleContract() {
  const document = await readFile(contractPath, "utf8");
  const match = document.match(/```space-role-contract\n([\s\S]*?)\n```/);
  assert.ok(match, "spaces contract must expose a role contract surface");
  return JSON.parse(match[1]);
}

test("T21-T31: roles grant governance capability but never channel visibility", async () => {
  const contract = await roleContract();

  assert.deepEqual(
    contract.roles.map(({ name }) => name),
    ["member", "moderator", "admin"],
  );
  assert.equal(contract.custom_roles, false);
  assert.equal(contract.visibility.basis, "channel_membership_and_current_key_possession");
  assert.equal(contract.visibility.role_grants_channel_visibility, false);
  assert.equal(contract.visibility.role_grants_history, false);
  assert.equal(contract.visibility.owner_or_recovery_key_exists, false);

  const permitted = new Set(["moderate_members", "manage_roles", "manage_channels"]);
  for (const role of contract.roles) {
    for (const capability of role.governance_capabilities) {
      assert.ok(permitted.has(capability), `${role.name} has an invalid capability: ${capability}`);
    }
  }
});
