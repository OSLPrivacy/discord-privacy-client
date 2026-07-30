import { describe, expect, it } from "vitest";

import {
  AUTOSCRUB_BASE_CONTRACT,
  AUTOSCRUB_UNATTENDED_BASE_CONTRACT,
  autoscrubUnattendedContractGate,
  createAutoscrubUnattendedContract,
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

describe("autoscrub-contract.ts", () => {
  it("keeps unattended cleanup contracts refused by default and deeply frozen after parsing", () => {
    expect(AUTOSCRUB_UNATTENDED_BASE_CONTRACT).toEqual({
      production: true,
      unattendedAllowed: false,
      reviewRequiredEveryBatch: true,
      externalSecurityReviewPassed: false,
    });
    expect(AUTOSCRUB_BASE_CONTRACT).toBe(AUTOSCRUB_UNATTENDED_BASE_CONTRACT);
    expect(Object.isFrozen(AUTOSCRUB_UNATTENDED_BASE_CONTRACT)).toBe(true);
    expect(autoscrubUnattendedContractGate(AUTOSCRUB_BASE_CONTRACT)).toEqual({
      state: "refused",
      reason: "unattended-disabled",
    });
    expect(() => {
      (AUTOSCRUB_UNATTENDED_BASE_CONTRACT as { unattendedAllowed: boolean }).unattendedAllowed = true;
    }).toThrow(TypeError);

    const ready = createAutoscrubUnattendedContract({
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    });
    expect(ready).toEqual(readyContract);
    expect(Object.isFrozen(ready)).toBe(true);
    expect(autoscrubUnattendedContractGate(ready)).toEqual({
      state: "ready",
      command: "autoscrub_unattended_run",
    });
    expect(() => {
      (ready as { reviewRequiredEveryBatch: boolean }).reviewRequiredEveryBatch = true;
    }).toThrow(TypeError);

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
    }).toThrow(TypeError);

    const parsedFleet = parseAutoScrubFleetStatus(fleetStatus);
    expect(Object.isFrozen(parsedFleet)).toBe(true);
    expect(Object.isFrozen(parsedFleet.quitGuard)).toBe(true);
    expect(Object.isFrozen(parsedFleet.runs)).toBe(true);
    expect(Object.isFrozen(parsedFleet.runs[0])).toBe(true);
    expect(() => {
      (parsedFleet.runs as unknown[]).push({ ...fleetStatus.runs[0], runId: "run-002" });
    }).toThrow(TypeError);
    expect(() => {
      (parsedFleet.runs[0] as { mutationAllowed: boolean }).mutationAllowed = true;
    }).toThrow(TypeError);
    expect(parsedFleet.runs).toHaveLength(1);
    expect(parsedFleet.runs[0].mutationAllowed).toBe(false);
  });
});
