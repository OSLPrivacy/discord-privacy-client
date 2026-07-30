import { describe, expect, it } from "vitest";
import {
  AUTOSCRUB_BASE_CONTRACT,
  autoscrubUnattendedContractGate,
  parseAutoScrubFleetStatus,
  parseAutoscrubUnattendedContract,
} from "./autoscrub-contract";

const readyContract = {
  production: true,
  unattendedAllowed: true,
  reviewRequiredEveryBatch: false,
  externalSecurityReviewPassed: true,
};

const fleetStatus = {
  contract: "autoscrubRunFleet.v1",
  openRunCount: 1,
  globalStopRequested: false,
  unattendedExecutionAllowed: false,
  quitGuard: {
    state: "notRequested",
    honestRemainingSecondsEstimate: null,
    reason: "No stop request is active.",
  },
  runs: [{
    runId: "run-001",
    serviceId: "discord",
    phase: "running",
    reviewedItemCount: 3,
    remainingItemCount: 2,
    stopRequested: false,
    mutationAllowed: false,
    lastOutcome: "prepared",
  }],
} as const;

describe("autoscrub contract hardening", () => {
  it("autoscrub-contract.ts frozen base contract + deep-freeze hardening of", () => {
    expect(Object.isFrozen(AUTOSCRUB_BASE_CONTRACT)).toBe(true);
    expect(autoscrubUnattendedContractGate(AUTOSCRUB_BASE_CONTRACT)).toEqual({
      state: "refused",
      reason: "unattended-disabled",
    });

    const raw = { ...readyContract };
    const parsed = parseAutoscrubUnattendedContract(raw);
    expect(parsed).toEqual(readyContract);
    expect(parsed).not.toBe(raw);
    expect(Object.isFrozen(parsed)).toBe(true);
    raw.unattendedAllowed = false;
    expect(autoscrubUnattendedContractGate(parsed)).toEqual({
      state: "ready",
      command: "autoscrub_unattended_run",
    });
    expect(() => {
      (parsed as { unattendedAllowed: boolean }).unattendedAllowed = false;
    }).toThrow();
    expect(autoscrubUnattendedContractGate(parsed)).toEqual({
      state: "ready",
      command: "autoscrub_unattended_run",
    });

    const status = parseAutoScrubFleetStatus(fleetStatus);
    expect(Object.isFrozen(status)).toBe(true);
    expect(Object.isFrozen(status.quitGuard)).toBe(true);
    expect(Object.isFrozen(status.runs)).toBe(true);
    expect(Object.isFrozen(status.runs[0])).toBe(true);
    expect(() => {
      (status.runs as unknown[]).push({ ...fleetStatus.runs[0], runId: "run-002" });
    }).toThrow();
    expect(() => {
      (status.runs[0] as { mutationAllowed: boolean }).mutationAllowed = true;
    }).toThrow();
    expect(status.runs).toHaveLength(1);
    expect(status.runs[0].mutationAllowed).toBe(false);
  });
});
