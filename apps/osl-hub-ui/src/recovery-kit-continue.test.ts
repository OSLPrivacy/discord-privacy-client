/**
 * NEW-1 — "I saved my recovery kit" + Continue looped back to the gate.
 *
 * Reproduced 2/2 against the real app: with the checkbox visibly ticked, one
 * press of Continue re-rendered the recovery step reading "You never confirmed
 * saving your recovery kit". `recovery_kit_status.json` was written at that
 * instant, so persistence was never the problem — the route was re-derived from
 * an in-memory mirror that only moved after the native write came back.
 *
 * These tests drive the real reducer, the real resume policy and the real flag
 * against a native write that is deliberately still in flight, which is the
 * only condition under which the defect appears.
 */

import { describe, expect, it } from "vitest";
import { createRecoveryKitUnsavedFlag } from "./recovery-kit-flag";
import {
  initialRecoveryKitState,
  recoveryKitReducer,
  recoveryKitView,
  type RecoveryKitState,
} from "./recovery-kit";
import {
  RECOVERY_KIT_UNSAVED_STORAGE_KEY,
  resumeOnboardingRoute,
  type OnboardingResumeStorage,
} from "./onboarding-resume";

const RESUME_KEY = "osl-onboarding-resume-v1";

function memoryStorage(seed: Record<string, string> = {}): OnboardingResumeStorage & { map: Map<string, string> } {
  const map = new Map(Object.entries(seed));
  return {
    map,
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => { map.set(key, value); },
    removeItem: (key) => { map.delete(key); },
  };
}

/** A native write the test decides when — and in which order — to settle. */
function pausedWrites() {
  const settle: Array<() => void> = [];
  const order: boolean[] = [];
  return {
    order,
    write: (unsaved: boolean): Promise<boolean> => new Promise((resolve) => {
      settle.push(() => { order.push(unsaved); resolve(true); });
    }),
    pending: () => settle.length,
    settleAll: () => { for (const fire of settle.splice(0)) fire(); },
    settleLast: () => { settle.pop()?.(); },
    settleFirst: () => { settle.shift()?.(); },
  };
}

/** The screen as it stands the moment the kit is on display. */
function kitOnScreen(kitUnsaved: boolean): RecoveryKitState {
  return {
    ...initialRecoveryKitState(
      { userId: "osl-user", identityPhrase: "twelve words of identity phrase here now", passwordPhrase: "twelve words of password phrase here now" },
      kitUnsaved,
    ),
    captureProven: true,
    captureEnforcement: "enforced",
  };
}

describe("confirming a saved recovery kit is believed immediately", () => {
  it("does not re-derive the route from a mirror the native write has not updated yet", async () => {
    const storage = memoryStorage({ [RECOVERY_KIT_UNSAVED_STORAGE_KEY]: "1" });
    const native = pausedWrites();
    const flag = createRecoveryKitUnsavedFlag({
      storage,
      read: async () => true,
      write: native.write,
    });
    await flag.load();
    expect(flag.unsaved()).toBe(true);

    let state = kitOnScreen(flag.unsaved());
    expect(recoveryKitView(state).mode).toBe("kit");

    // Tick "I saved my recovery kit".
    state = recoveryKitReducer(state, { kind: "set-saved-acknowledged", acknowledged: true }).state;
    void flag.set(state.kitUnsaved);

    // Press Continue.
    const pressed = recoveryKitReducer(state, { kind: "continue" });
    expect(pressed.outcome).toBe("leave-recovery");
    void flag.set(pressed.state.kitUnsaved);

    // Everything below happens on the same turn as the press, before any
    // native write has been allowed to settle. This is the defect: the router
    // ran here and still saw "unsaved".
    expect(native.order).toEqual([]);
    expect(flag.unsaved()).toBe(false);
    expect(resumeOnboardingRoute(storage, RESUME_KEY)).not.toBe("recovery");

    // And the screen the owner lands on is not the gate accusing them.
    const afterContinue = { ...pressed.state, kitUnsaved: flag.unsaved() };
    expect(recoveryKitView(afterContinue).mode).not.toBe("reveal-required");

    native.settleAll();
  });

  it("keeps the owner on their pending setup step rather than restarting it", async () => {
    const storage = memoryStorage({
      [RECOVERY_KIT_UNSAVED_STORAGE_KEY]: "1",
      [RESUME_KEY]: "passwords",
    });
    const native = pausedWrites();
    const flag = createRecoveryKitUnsavedFlag({ storage, read: async () => true, write: native.write });
    await flag.load();

    let state = kitOnScreen(flag.unsaved());
    state = recoveryKitReducer(state, { kind: "set-saved-acknowledged", acknowledged: true }).state;
    const pressed = recoveryKitReducer(state, { kind: "continue" });
    void flag.set(pressed.state.kitUnsaved);

    expect(resumeOnboardingRoute(storage, RESUME_KEY)).toBe("passwords");
    native.settleAll();
  });

  it("does not let the checkbox's write land after Continue's and re-arm the gate", async () => {
    const storage = memoryStorage({ [RECOVERY_KIT_UNSAVED_STORAGE_KEY]: "1" });
    const native = pausedWrites();
    const flag = createRecoveryKitUnsavedFlag({ storage, read: async () => true, write: native.write });
    await flag.load();

    let state = kitOnScreen(flag.unsaved());
    state = recoveryKitReducer(state, { kind: "set-saved-acknowledged", acknowledged: true }).state;
    const checkbox = flag.set(state.kitUnsaved);
    const pressed = recoveryKitReducer(state, { kind: "continue" });
    const cont = flag.set(pressed.state.kitUnsaved);

    // Settle in the worst order the runtime allows: the older write last.
    // Serialisation means only one write can be outstanding at a time, so
    // draining them repeatedly still yields call order on disk.
    for (let tick = 0; tick < 8; tick += 1) {
      native.settleLast();
      await Promise.resolve();
    }
    await Promise.all([checkbox, cont]);

    // The last value the native side saw must be "saved".
    expect(native.order.at(-1)).toBe(false);
    expect(flag.unsaved()).toBe(false);
  });

  it("still refuses Continue when the box was never ticked", () => {
    const state = kitOnScreen(true);
    const pressed = recoveryKitReducer(state, { kind: "continue" });
    expect(pressed.outcome).toBe("rejected");
    expect(pressed.state.kitUnsaved).toBe(true);
  });

  it("TASK0334 advances only for the explicit no-secret state and records that choice", () => {
    const withoutNoSecretState = initialRecoveryKitState(null, true);
    const without = recoveryKitReducer(withoutNoSecretState, { kind: "continue" });
    console.log(
      `TASK0334 without.state=${recoveryKitView(withoutNoSecretState).mode} outcome=${without.outcome} recorded=${without.state.noRecoverySecretAcknowledged}`,
    );

    expect(recoveryKitView(withoutNoSecretState).mode).toBe("reveal-required");
    expect(without.outcome).toBe("rejected");
    expect(without.state.noRecoverySecretAcknowledged).toBe(false);

    const explicitNoSecretState = initialRecoveryKitState(null, false);
    const withNoSecret = recoveryKitReducer(explicitNoSecretState, { kind: "continue" });
    console.log(
      `TASK0334 with.state=${recoveryKitView(explicitNoSecretState).mode} outcome=${withNoSecret.outcome} recorded=${withNoSecret.state.noRecoverySecretAcknowledged}`,
    );

    expect(recoveryKitView(explicitNoSecretState).mode).toBe("unavailable");
    expect(withNoSecret.outcome).toBe("leave-recovery");
    expect(withNoSecret.state.noRecoverySecretAcknowledged).toBe(true);
  });

  it("lets the owner through this session even when the native write fails", async () => {
    const storage = memoryStorage({ [RECOVERY_KIT_UNSAVED_STORAGE_KEY]: "1" });
    const flag = createRecoveryKitUnsavedFlag({
      storage,
      read: async () => true,
      write: async () => { throw new Error("disk is gone"); },
    });
    await flag.load();

    let state = kitOnScreen(flag.unsaved());
    state = recoveryKitReducer(state, { kind: "set-saved-acknowledged", acknowledged: true }).state;
    const pressed = recoveryKitReducer(state, { kind: "continue" });
    await expect(flag.set(pressed.state.kitUnsaved)).resolves.toBe(false);
    expect(flag.unsaved()).toBe(false);
    expect(resumeOnboardingRoute(storage, RESUME_KEY)).not.toBe("recovery");
  });

  it("keeps the deferral honest: Remind me later leaves the kit outstanding", async () => {
    const storage = memoryStorage();
    const flag = createRecoveryKitUnsavedFlag({ storage, read: async () => false, write: async () => true });
    await flag.load();

    const state = kitOnScreen(flag.unsaved());
    const deferred = recoveryKitReducer(state, { kind: "remind-me-later" });
    expect(deferred.outcome).toBe("leave-recovery");
    void flag.set(deferred.state.kitUnsaved);

    expect(flag.unsaved()).toBe(true);
    expect(resumeOnboardingRoute(storage, RESUME_KEY)).toBe("recovery");
  });

  it("does not drop the local mark when the native answer cannot be read", async () => {
    const storage = memoryStorage({ [RECOVERY_KIT_UNSAVED_STORAGE_KEY]: "1" });
    const flag = createRecoveryKitUnsavedFlag({ storage, read: async () => null, write: async () => true });
    await flag.load();

    // The only surviving hint that a kit was never saved must still route back
    // to the step.
    expect(resumeOnboardingRoute(storage, RESUME_KEY)).toBe("recovery");
  });

  it("TASK0334 advances only from the explicit no-secret state and records that choice", () => {
    const without = recoveryKitReducer(initialRecoveryKitState(null, true), { kind: "continue" });
    const withNoSecret = recoveryKitReducer(initialRecoveryKitState(null, false), { kind: "continue" });

    console.log(
      `TASK0334 without.state=${recoveryKitView(without.state).mode} outcome=${without.outcome} recorded=${without.state.noRecoverySecretAcknowledged}`,
    );
    console.log(
      `TASK0334 with.state=${recoveryKitView(withNoSecret.state).mode} outcome=${withNoSecret.outcome} recorded=${withNoSecret.state.noRecoverySecretAcknowledged}`,
    );

    expect(recoveryKitView(without.state).mode).toBe("reveal-required");
    expect(without.outcome).toBe("rejected");
    expect(without.state.noRecoverySecretAcknowledged).toBe(false);
    expect(recoveryKitView(withNoSecret.state).mode).toBe("unavailable");
    expect(withNoSecret.outcome).toBe("leave-recovery");
    expect(withNoSecret.state.noRecoverySecretAcknowledged).toBe(true);
  });
});
