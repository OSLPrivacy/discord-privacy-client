import { describe, expect, it } from "vitest";
import {
  acknowledgementAccepted,
  initialRecoveryKitState,
  RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT,
  recoveryKitReducer,
  recoveryKitView,
  visibleRecoverySecrets,
  type RecoveryKitAction,
  type RecoveryKitSecrets,
  type RecoveryKitState,
} from "./recovery-kit";

const SECRETS: RecoveryKitSecrets = {
  userId: "osl-user-1",
  identityPhrase: "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima",
  passwordPhrase: "mike november oscar papa quebec romeo sierra tango uniform victor whiskey xray",
};

function unprovenOnWindows(): RecoveryKitState {
  return { ...initialRecoveryKitState(SECRETS, true), captureEnforcement: "enforced" };
}

function drive(state: RecoveryKitState, actions: RecoveryKitAction[]): RecoveryKitState {
  return actions.reduce((current, action) => recoveryKitReducer(current, action).state, state);
}

describe("T15-A7 the capture refusal is escapable", () => {
  it("offers a way forward that is not just Retry when protection cannot be proven", () => {
    const view = recoveryKitView(unprovenOnWindows());

    expect(view.mode).toBe("refusal");
    expect(view.secretsVisible).toBe(false);
    const exitsThatAreNotRetry = view.exits.filter((exit) => exit.id !== "retry-protection");
    expect(exitsThatAreNotRetry.length).toBeGreaterThan(0);
    expect(view.exits.map((exit) => exit.id)).toEqual(
      expect.arrayContaining(["show-anyway", "remind-me-later"]),
    );
  });

  it("keeps the phrases when a wrong acknowledgement is refused", () => {
    const refused = recoveryKitReducer(unprovenOnWindows(), {
      kind: "show-anyway",
      acknowledgement: "yes",
    });

    expect(refused.outcome).toBe("rejected");
    expect(recoveryKitView(refused.state).secretsVisible).toBe(false);
    // The whole point: refusing the action must never be a way to destroy a
    // secret the owner has not written down yet.
    expect(refused.state.secrets).toEqual(SECRETS);
  });

  it("shows the same phrases the owner would otherwise have lost once acknowledged", () => {
    const before = unprovenOnWindows();
    expect(visibleRecoverySecrets(before)).toBeNull();

    const after = recoveryKitReducer(before, {
      kind: "show-anyway",
      acknowledgement: RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT,
    });

    expect(after.outcome).toBe("none");
    expect(recoveryKitView(after.state).mode).toBe("kit");
    expect(visibleRecoverySecrets(after.state)).toEqual(SECRETS);
  });

  it("accepts the acknowledgement regardless of case and padding, and nothing else", () => {
    expect(acknowledgementAccepted("  SHOW Anyway  ")).toBe(true);
    expect(acknowledgementAccepted("show")).toBe(false);
    expect(acknowledgementAccepted("")).toBe(false);
  });

  it("lets the owner defer without pretending the kit was saved", () => {
    const deferred = recoveryKitReducer(unprovenOnWindows(), { kind: "remind-me-later" });

    expect(deferred.outcome).toBe("leave-recovery");
    expect(deferred.state.kitUnsaved).toBe(true);
    expect(deferred.state.secrets).toBeNull();
    expect(recoveryKitView(deferred.state).mode).toBe("reveal-required");
  });

  it("refuses Continue until the owner says they saved the kit, then clears the flag", () => {
    const shown = drive(unprovenOnWindows(), [
      { kind: "show-anyway", acknowledgement: RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT },
    ]);

    const premature = recoveryKitReducer(shown, { kind: "continue" });
    expect(premature.outcome).toBe("rejected");
    expect(premature.state.kitUnsaved).toBe(true);

    const acknowledged = recoveryKitReducer(shown, {
      kind: "set-saved-acknowledged",
      acknowledged: true,
    }).state;
    const finished = recoveryKitReducer(acknowledged, { kind: "continue" });

    expect(finished.outcome).toBe("leave-recovery");
    expect(finished.state.kitUnsaved).toBe(false);
    expect(finished.state.secrets).toBeNull();
  });

  it("asks for the kit to be re-read when it is outstanding but no longer in memory", () => {
    const view = recoveryKitView(initialRecoveryKitState(null, true));

    expect(view.mode).toBe("reveal-required");
    // The reveal is offered first, but never alone: it depends on a backend
    // round trip, and this screen is reached on every launch until the kit is
    // saved, so a round trip that does not come back must not be the only way
    // off it. See recovery-reveal-deadend.test.ts for the hang itself.
    expect(view.exits.map((exit) => exit.id)).toEqual(["reveal-with-password", "remind-me-later"]);
    expect(view.exits.every((exit) => !exit.requiresAcknowledgement)).toBe(true);
  });

  it("stays out of the way when there is no kit and nothing outstanding", () => {
    expect(recoveryKitView(initialRecoveryKitState(null, false)).mode).toBe("unavailable");
  });

  it("puts the phrases back on screen after the backend re-reads them", () => {
    const revealed = recoveryKitReducer(initialRecoveryKitState(null, true), {
      kind: "revealed",
      secrets: { userId: SECRETS.userId, identityPhrase: null, passwordPhrase: SECRETS.passwordPhrase },
    });
    const shown = recoveryKitReducer(revealed.state, {
      kind: "show-anyway",
      acknowledgement: RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT,
    }).state;

    expect(visibleRecoverySecrets(shown)?.passwordPhrase).toBe(SECRETS.passwordPhrase);
  });
});

describe("T15 the capture-resistance claim tracks the platform, not the return value", () => {
  it("does not claim capture resistance where the platform enforces none", () => {
    // The Linux/macOS shape: the protection call "succeeded" (it is a no-op
    // stub that always does) so the latch is proven, but nothing is protected.
    const proven = recoveryKitReducer(initialRecoveryKitState(SECRETS, true), {
      kind: "protection-proved",
      proven: true,
      enforcement: "unenforced",
    }).state;
    const view = recoveryKitView(proven);

    expect(view.secretsVisible).toBe(true);
    expect(view.claim).toBe("not-capture-resistant");
    expect(view.notice).toMatch(/capturable/i);
  });

  it("claims capture resistance only when the platform enforces it and the latch is proven", () => {
    const proven = recoveryKitReducer(initialRecoveryKitState(SECRETS, true), {
      kind: "protection-proved",
      proven: true,
      enforcement: "enforced",
    }).state;

    expect(recoveryKitView(proven).claim).toBe("capture-resistant");
  });

  it("does not offer Retry where the protection primitive does not exist at all", () => {
    const view = recoveryKitView(initialRecoveryKitState(SECRETS, true));

    expect(view.mode).toBe("refusal");
    expect(view.claim).toBe("not-capture-resistant");
    expect(view.exits.map((exit) => exit.id)).not.toContain("retry-protection");
    expect(view.exits.map((exit) => exit.id)).toEqual(
      expect.arrayContaining(["show-anyway", "remind-me-later"]),
    );
  });
});
