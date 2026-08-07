import { describe, expect, it } from "vitest";
import {
  addReadyToScanFile,
  buildReadyToScanRunPlan,
  initialReadyToScanState,
  readyToScanMarkup,
  removeReadyToScanFile,
  toggleReadyToScanAccount,
  toggleReadyToScanRule,
} from "./ready-to-scan-screen";

function fixtureState() {
  return initialReadyToScanState({
    runId: "task-1418-run",
    accounts: [
      { serviceId: "discord", accountId: "account-a" },
      { serviceId: "discord", accountId: "account-b" },
      { serviceId: "gmail", accountId: "account-c" },
    ],
    rules: [
      { id: "passwords_and_codes", label: "Passwords and codes" },
      { id: "personal_details", label: "Personal details" },
      { id: "money_details", label: "Money details" },
    ],
    watchView: "watch-live",
  });
}

describe("Ready-to-scan page", () => {
  it("shows approved accounts, chosen rules, watch, Home, optional file scan, Back, and Start", () => {
    const markup = readyToScanMarkup(fixtureState());
    expect(markup).toContain("Approved accounts");
    expect(markup).toContain("Chosen rules");
    expect(markup).toContain("Watch live");
    expect(markup).toContain("data-ready-to-scan-home");
    expect(markup).toContain("Optional file scan");
    expect(markup).toContain("data-ready-to-scan-back");
    expect(markup).toContain("data-ready-to-scan-start");
  });

  it("renders one row per approved account and one per chosen rule", () => {
    const state = fixtureState();
    const markup = readyToScanMarkup(state);
    expect([...markup.matchAll(/data-ready-to-scan-account-toggle="[^"]+"/gu)]).toHaveLength(3);
    expect([...markup.matchAll(/data-ready-to-scan-rule-toggle="[^"]+"/gu)]).toHaveLength(3);
  });

  it("Start creates a run plan whose account list, scope, and batch size equal the visible choices exactly", () => {
    const state = fixtureState();
    const plan = buildReadyToScanRunPlan(state);
    expect(plan.accountList).toEqual([
      { serviceId: "discord", accountId: "account-a" },
      { serviceId: "discord", accountId: "account-b" },
      { serviceId: "gmail", accountId: "account-c" },
    ]);
    expect(plan.scope).toEqual(["passwords_and_codes", "personal_details", "money_details"]);
    expect(plan.batchSize).toBe(0);
  });

  it("unticking one approved account changes only the account list field", () => {
    const before = fixtureState();
    const beforePlan = buildReadyToScanRunPlan(before);

    const after = toggleReadyToScanAccount(before, "discord", "account-b");
    const afterPlan = buildReadyToScanRunPlan(after);

    expect(afterPlan.accountList).toEqual([
      { serviceId: "discord", accountId: "account-a" },
      { serviceId: "gmail", accountId: "account-c" },
    ]);
    expect(afterPlan.accountList).not.toEqual(beforePlan.accountList);
    expect(afterPlan.scope).toEqual(beforePlan.scope);
    expect(afterPlan.batchSize).toBe(beforePlan.batchSize);
  });

  it("unticking one chosen rule changes only the scope field", () => {
    const before = fixtureState();
    const beforePlan = buildReadyToScanRunPlan(before);

    const after = toggleReadyToScanRule(before, "personal_details");
    const afterPlan = buildReadyToScanRunPlan(after);

    expect(afterPlan.scope).toEqual(["passwords_and_codes", "money_details"]);
    expect(afterPlan.scope).not.toEqual(beforePlan.scope);
    expect(afterPlan.accountList).toEqual(beforePlan.accountList);
    expect(afterPlan.batchSize).toBe(beforePlan.batchSize);
  });

  it("adding one optional file changes only the batch size field", () => {
    const before = fixtureState();
    const beforePlan = buildReadyToScanRunPlan(before);

    const after = addReadyToScanFile(before, "/tmp/task-1418-export.txt");
    const afterPlan = buildReadyToScanRunPlan(after);

    expect(afterPlan.batchSize).toBe(1);
    expect(afterPlan.batchSize).not.toBe(beforePlan.batchSize);
    expect(afterPlan.accountList).toEqual(beforePlan.accountList);
    expect(afterPlan.scope).toEqual(beforePlan.scope);
  });

  it("removing an optional file changes only the batch size field back down", () => {
    const withFile = addReadyToScanFile(fixtureState(), "/tmp/task-1418-export.txt");
    const withFilePlan = buildReadyToScanRunPlan(withFile);

    const removed = removeReadyToScanFile(withFile, "/tmp/task-1418-export.txt");
    const removedPlan = buildReadyToScanRunPlan(removed);

    expect(removedPlan.batchSize).toBe(0);
    expect(removedPlan.batchSize).not.toBe(withFilePlan.batchSize);
    expect(removedPlan.accountList).toEqual(withFilePlan.accountList);
    expect(removedPlan.scope).toEqual(withFilePlan.scope);
  });

  it("toggling a second, different account does not resurrect the first toggle", () => {
    const step1 = toggleReadyToScanAccount(fixtureState(), "discord", "account-a");
    const step2 = toggleReadyToScanAccount(step1, "gmail", "account-c");
    const plan = buildReadyToScanRunPlan(step2);
    expect(plan.accountList).toEqual([{ serviceId: "discord", accountId: "account-b" }]);
  });

  it("carries the watch choice from the earlier screen without exposing it as an editable control", () => {
    const state = initialReadyToScanState({
      runId: "task-1418-run-background",
      accounts: [{ serviceId: "discord", accountId: "account-a" }],
      rules: [{ id: "everything_above", label: "Everything above" }],
      watchView: "run-in-background",
    });
    const markup = readyToScanMarkup(state);
    expect(markup).toContain("Run in background");
    expect(markup).not.toContain('data-ready-to-scan-watch-toggle');
    expect(buildReadyToScanRunPlan(state).watchView).toBe("run-in-background");
  });
});
