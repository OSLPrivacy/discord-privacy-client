import { describe, expect, it } from "vitest";
import {
  completeOldDeviceCopyDecision,
  initialOldDeviceCopyDecision,
  oldDeviceCopyDecisionView,
  selectOldDeviceCopyAction,
} from "./device-transfer-source";

describe("T15-E5 old-device transfer decision", () => {
  it("cannot complete the confirmed-import flow until the owner explicitly keeps or destroys the old copy", () => {
    const awaitingChoice = initialOldDeviceCopyDecision({ importConfirmed: true });

    expect(awaitingChoice.choice).toBeNull();
    expect(oldDeviceCopyDecisionView(awaitingChoice).options.map((option) => option.id)).toEqual([
      "keep",
      "destroy",
    ]);
    expect(completeOldDeviceCopyDecision(awaitingChoice)).toMatchObject({
      outcome: "rejected",
      state: awaitingChoice,
    });

    const kept = selectOldDeviceCopyAction(awaitingChoice, "keep").state;
    expect(completeOldDeviceCopyDecision(kept)).toMatchObject({ outcome: "complete", choice: "keep" });

    const destroyed = selectOldDeviceCopyAction(awaitingChoice, "destroy").state;
    expect(completeOldDeviceCopyDecision(destroyed)).toMatchObject({ outcome: "complete", choice: "destroy" });
  });

  it("refuses to present or decide anything until the destination import is confirmed", () => {
    const unconfirmed = initialOldDeviceCopyDecision({ importConfirmed: false });

    expect(oldDeviceCopyDecisionView(unconfirmed).mode).toBe("unavailable");
    expect(selectOldDeviceCopyAction(unconfirmed, "keep")).toMatchObject({
      outcome: "rejected",
      state: unconfirmed,
    });
  });

  it("rejects a malformed third option without changing the pending decision", () => {
    const awaitingChoice = initialOldDeviceCopyDecision({ importConfirmed: true });

    expect(selectOldDeviceCopyAction(awaitingChoice, "later")).toEqual({
      state: awaitingChoice,
      outcome: "rejected",
    });
  });
});
