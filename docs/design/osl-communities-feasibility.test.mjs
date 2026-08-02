import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const feasibilityPath = new URL("./osl-communities-feasibility.md", import.meta.url);
const source = readFileSync(feasibilityPath, "utf8");
const match = source.match(/## Contract vector[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the feasibility record must expose a machine-checkable vector");
const contract = JSON.parse(match[1]);

function fanoutRows({ members, devicesPerMember, manifestRows }) {
  return members * devicesPerMember + manifestRows;
}

test("community fan-out accounts for every recipient device and the manifest", () => {
  assert.equal(
    fanoutRows(contract.fanout),
    contract.fanout.totalRows,
  );
  assert.equal(
    Math.floor(contract.fanout.globalLiveRowBudget / contract.fanout.totalRows),
    contract.fanout.messagesBeforeBudgetExhaustion,
  );
});

test("the scoped community product uses the owner-selected Enclaves name", () => {
  assert.equal(contract.product, "osl-enclaves");
});

test("the offline worked case cannot be sized as one copy per member", () => {
  const { offlineMembers, devicesPerMember, messagesPerDay, undeliveredRows } =
    contract.offlineWorkedCase;
  assert.equal(offlineMembers * devicesPerMember * messagesPerDay, undeliveredRows);
  assert.ok(undeliveredRows > offlineMembers * messagesPerDay);
});

test("the delivery estimate includes a real integration band beyond task sums", () => {
  assert.equal(contract.estimate.tasks, 73);
  assert.equal(contract.estimate.taskMinutes, 5290);
  assert.ok(contract.estimate.deliveryHoursLower > contract.estimate.taskHours);
  assert.ok(contract.estimate.deliveryHoursUpper >= contract.estimate.deliveryHoursLower);
  assert.equal(contract.estimate.windowsVmProofTasks, 4);
});
