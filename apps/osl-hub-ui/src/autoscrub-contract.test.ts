import { describe, expect, it } from "vitest";

import {
  AUTOSCRUB_UNATTENDED_BASE_CONTRACT,
  autoscrubUnattendedContractGate,
  createAutoscrubUnattendedContract,
  parseAutoScrubFleetStatus,
  parseAutoscrubUnattendedContract,
} from "./autoscrub-contract";

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
  it("autoscrub-contract.ts frozen base contract + deep-freeze hardening of parsed contracts", () => {
    expect(AUTOSCRUB_UNATTENDED_BASE_CONTRACT).toEqual({
      production: true,
      unattendedAllowed: false,
      reviewRequiredEveryBatch: true,
      externalSecurityReviewPassed: false,
    });
    expect(Object.isFrozen(AUTOSCRUB_UNATTENDED_BASE_CONTRACT)).toBe(true);
    expect(autoscrubUnattendedContractGate(AUTOSCRUB_UNATTENDED_BASE_CONTRACT)).toEqual({
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
    expect(Object.isFrozen(ready)).toBe(true);
    expect(autoscrubUnattendedContractGate(ready)).toEqual({
      state: "ready",
      command: "autoscrub_unattended_run",
    });
    expect(() => {
      (ready as { reviewRequiredEveryBatch: boolean }).reviewRequiredEveryBatch = true;
    }).toThrow(TypeError);

    const parsedContract = parseAutoscrubUnattendedContract({
      production: true,
      unattendedAllowed: true,
      reviewRequiredEveryBatch: false,
      externalSecurityReviewPassed: true,
    });
    expect(parsedContract).not.toBeNull();
    expect(Object.isFrozen(parsedContract)).toBe(true);

    const parsedFleet = parseAutoScrubFleetStatus(fleetStatus);
    expect(Object.isFrozen(parsedFleet)).toBe(true);
    expect(Object.isFrozen(parsedFleet.quitGuard)).toBe(true);
    expect(Object.isFrozen(parsedFleet.runs)).toBe(true);
    expect(Object.isFrozen(parsedFleet.runs[0])).toBe(true);
    expect(() => {
      (parsedFleet.runs[0] as { mutationAllowed: boolean }).mutationAllowed = true;
    }).toThrow(TypeError);
  });
});
