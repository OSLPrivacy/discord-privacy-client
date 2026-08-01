import { describe, expect, it } from "vitest";
import {
  RECOVERY_REVEAL_FAILED_ERROR,
  RECOVERY_REVEAL_NO_PASSWORD_ERROR,
  RECOVERY_REVEAL_UNANSWERED_ERROR,
  RECOVERY_REVEAL_WRONG_PASSWORD_ERROR,
  runRecoveryReveal,
  submitsRecoveryReveal,
  type RecoveryRevealOutcome,
} from "./recovery-reveal";
import { onboardingPaintDecision } from "./ui-behavior";
import {
  initialRecoveryKitState,
  recoveryKitReducer,
  recoveryKitView,
  type RecoveryKitState,
} from "./recovery-kit";

/**
 * A deadline this test can fire on demand. `clearTimer` neutralises the entry
 * rather than removing it, so `fireAll()` after a completed reveal is a no-op
 * and a leaked timer would show up as an unexpected extra outcome.
 */
function controllableTimers() {
  const pending: Array<() => void> = [];
  return {
    timers: {
      deadlineMs: 20_000,
      setTimer: (fire: () => void) => pending.push(fire) - 1,
      clearTimer: (handle: unknown) => { pending[handle as number] = () => {}; },
    },
    armed: () => pending.length,
    fireAll: () => { for (const fire of [...pending]) fire(); },
  };
}

async function settleMicrotasks(): Promise<void> {
  for (let tick = 0; tick < 40; tick += 1) await Promise.resolve();
}

/** The mutable screen state the reveal form owns, and a log of every paint. */
function screen(overrides: {
  proveCaptureProtection?: () => Promise<unknown>;
  readRecoveryPhrase?: (password: string) => Promise<string | null>;
}) {
  const state = { busy: false, error: null as string | null, paints: 0, busyAtPaint: [] as boolean[] };
  return {
    state,
    actions: {
      isBusy: () => state.busy,
      setBusy: (busy: boolean) => { state.busy = busy; },
      setError: (message: string | null) => { state.error = message; },
      render: () => { state.paints += 1; state.busyAtPaint.push(state.busy); },
      proveCaptureProtection: overrides.proveCaptureProtection ?? (async () => true),
      readRecoveryPhrase: overrides.readRecoveryPhrase ?? (async () => "twelve word password recovery phrase"),
    },
  };
}

const NEVER: Promise<never> = new Promise(() => {});

describe("the recovery reveal always ends", () => {
  it("gives up on a reveal that never settles, and drops the busy latch", async () => {
    const clock = controllableTimers();
    const ui = screen({ readRecoveryPhrase: () => NEVER });

    const flow = runRecoveryReveal("correct horse", ui.actions, clock.timers);
    await settleMicrotasks();
    // Mid-flight: the press was acknowledged and the control is busy.
    expect(ui.state.busy).toBe(true);
    expect(ui.state.paints).toBeGreaterThan(0);

    clock.fireAll();
    const outcome = await flow;

    expect(outcome).toEqual({ kind: "unanswered" });
    expect(ui.state.busy).toBe(false);
    expect(ui.state.error).toBe(RECOVERY_REVEAL_UNANSWERED_ERROR);
    // The last paint carries the cleared latch, so the button is usable again.
    expect(ui.state.busyAtPaint.at(-1)).toBe(false);
  });

  it("lets the owner press again after a reveal that never settled", async () => {
    const clock = controllableTimers();
    const ui = screen({ readRecoveryPhrase: () => NEVER });

    const first = runRecoveryReveal("correct horse", ui.actions, clock.timers);
    await settleMicrotasks();
    clock.fireAll();
    await first;

    // Not swallowed by the "already running" guard: the latch really came down,
    // so a second press reaches the backend and can succeed.
    const second = await runRecoveryReveal(
      "correct horse",
      { ...ui.actions, readRecoveryPhrase: async () => "twelve word password recovery phrase" },
      clock.timers,
    );
    expect(second).toEqual({ kind: "revealed", passwordPhrase: "twelve word password recovery phrase" });
  });

  it("survives a capture proof that never settles", async () => {
    const clock = controllableTimers();
    const ui = screen({ proveCaptureProtection: () => NEVER });

    const flow = runRecoveryReveal("correct horse", ui.actions, clock.timers);
    await settleMicrotasks();
    clock.fireAll();
    await settleMicrotasks();
    clock.fireAll();
    const outcome = await flow;

    expect(outcome).toEqual({ kind: "revealed", passwordPhrase: "twelve word password recovery phrase" });
    expect(ui.state.busy).toBe(false);
  });

  it("survives a capture proof that raises", async () => {
    const ui = screen({ proveCaptureProtection: async () => { throw new Error("no affinity"); } });
    const outcome = await runRecoveryReveal("correct horse", ui.actions);
    expect(outcome).toEqual({ kind: "revealed", passwordPhrase: "twelve word password recovery phrase" });
    expect(ui.state.busy).toBe(false);
  });

  it("shows a readable error when the reveal raises", async () => {
    const ui = screen({ readRecoveryPhrase: async () => { throw new Error("ipc closed"); } });
    const outcome = await runRecoveryReveal("correct horse", ui.actions);
    expect(outcome).toEqual({ kind: "failed" });
    expect(ui.state.busy).toBe(false);
    expect(ui.state.error).toBe(RECOVERY_REVEAL_FAILED_ERROR);
    // Nothing derived from the backend's own message reaches the screen.
    expect(ui.state.error).not.toContain("ipc closed");
  });

  it("shows a readable error when the password is refused", async () => {
    const ui = screen({ readRecoveryPhrase: async () => null });
    const outcome = await runRecoveryReveal("wrong", ui.actions);
    expect(outcome).toEqual({ kind: "rejected" });
    expect(ui.state.busy).toBe(false);
    expect(ui.state.error).toBe(RECOVERY_REVEAL_WRONG_PASSWORD_ERROR);
    expect(ui.state.paints).toBeGreaterThan(0);
  });

  it("does not sit silent when the button is pressed with an empty field", async () => {
    const ui = screen({ readRecoveryPhrase: async () => { throw new Error("must not be called"); } });
    const outcome = await runRecoveryReveal("", ui.actions);
    expect(outcome).toEqual({ kind: "no-password" });
    expect(ui.state.error).toBe(RECOVERY_REVEAL_NO_PASSWORD_ERROR);
    expect(ui.state.paints).toBeGreaterThan(0);
    expect(ui.state.busy).toBe(false);
  });

  it("clears the busy latch and repaints on every terminal outcome", async () => {
    const readers: Array<() => Promise<string | null>> = [
      async () => "phrase",
      async () => null,
      async () => { throw new Error("raised"); },
      () => NEVER,
    ];
    const seen: RecoveryRevealOutcome["kind"][] = [];
    for (const read of readers) {
      const clock = controllableTimers();
      const ui = screen({ readRecoveryPhrase: read });
      const flow = runRecoveryReveal("correct horse", ui.actions, clock.timers);
      await settleMicrotasks();
      clock.fireAll();
      const outcome = await flow;
      seen.push(outcome.kind);
      expect(ui.state.busy).toBe(false);
      expect(ui.state.busyAtPaint.at(-1)).toBe(false);
    }
    expect(seen).toEqual(["revealed", "rejected", "failed", "unanswered"]);
  });
});

describe("the recovery screen is never a dead end", () => {
  function revealRequired(): RecoveryKitState {
    // No secrets in memory and a kit the owner never confirmed saving: this is
    // the screen every launch lands on after "Remind me later".
    const state = initialRecoveryKitState(null, true);
    expect(recoveryKitView(state).mode).toBe("reveal-required");
    return state;
  }

  it("offers a way onward from every screen that holds secrets back", () => {
    const screens: RecoveryKitState[] = [
      revealRequired(),
      { ...initialRecoveryKitState({ userId: "u", identityPhrase: null, passwordPhrase: "p" }, true), captureEnforcement: "enforced" },
      { ...initialRecoveryKitState({ userId: "u", identityPhrase: null, passwordPhrase: "p" }, true), captureEnforcement: "unenforced" },
    ];
    for (const state of screens) {
      const view = recoveryKitView(state);
      expect(["reveal-required", "refusal"]).toContain(view.mode);
      // At least one listed exit actually leaves the recovery step, applied
      // exactly as the renderer would apply it, with no acknowledgement.
      const leaves = view.exits.filter((exit) => !exit.requiresAcknowledgement)
        .some((exit) => exit.id === "remind-me-later"
          && recoveryKitReducer(state, { kind: "remind-me-later" }).outcome === "leave-recovery");
      expect(leaves).toBe(true);
    }
  });

  it("keeps the escape reachable while a reveal is hung", async () => {
    const clock = controllableTimers();
    const ui = screen({ readRecoveryPhrase: () => NEVER });
    const state = revealRequired();

    const flow = runRecoveryReveal("correct horse", ui.actions, clock.timers);
    await settleMicrotasks();

    // Mid-hang, with the reveal control busy, the screen still lists an exit
    // that is not the reveal, and taking it leaves recovery without losing the
    // "kit still unsaved" flag that brings the owner back to it.
    expect(ui.state.busy).toBe(true);
    const exits = recoveryKitView(state).exits.map((exit) => exit.id);
    expect(exits).toContain("remind-me-later");
    const escaped = recoveryKitReducer(state, { kind: "remind-me-later" });
    expect(escaped.outcome).toBe("leave-recovery");
    expect(escaped.state.kitUnsaved).toBe(true);

    clock.fireAll();
    await flow;
  });
});

describe("the reveal form answers the keyboard", () => {
  it("submits on a bare Enter", () => {
    expect(submitsRecoveryReveal({ key: "Enter" })).toBe(true);
  });

  it("ignores Enter that belongs to something else", () => {
    expect(submitsRecoveryReveal({ key: "a" })).toBe(false);
    expect(submitsRecoveryReveal({ key: "Enter", isComposing: true })).toBe(false);
    expect(submitsRecoveryReveal({ key: "Enter", shiftKey: true })).toBe(false);
    expect(submitsRecoveryReveal({ key: "Enter", ctrlKey: true })).toBe(false);
    expect(submitsRecoveryReveal({ key: "Enter", altKey: true })).toBe(false);
    expect(submitsRecoveryReveal({ key: "Enter", metaKey: true })).toBe(false);
  });
});

describe("an answer to the owner's own keystroke is never deferred", () => {
  const typing = {
    markupUnchanged: false,
    shellMounted: true,
    sameRouteAsRendered: true,
    passwordEditInProgress: true,
  };

  it("still defers a background refresh while a password is being typed", () => {
    expect(onboardingPaintDecision({ ...typing, forced: false })).toBe("defer-sensitive-edit");
  });

  it("paints a forced result even though the password field still holds text", () => {
    // This is the reveal form's own outcome: the field it owns necessarily
    // still holds the typed password when the answer arrives, so an unforced
    // paint would be deferred and the screen would freeze mid-flow.
    expect(onboardingPaintDecision({ ...typing, forced: true })).toBe("paint");
    expect(onboardingPaintDecision({ ...typing, markupUnchanged: true, forced: true })).toBe("paint");
  });

  it("still short-circuits an unchanged repaint that nobody asked for", () => {
    expect(onboardingPaintDecision({
      markupUnchanged: true,
      shellMounted: true,
      sameRouteAsRendered: true,
      passwordEditInProgress: false,
      forced: false,
    })).toBe("skip-unchanged");
  });
});
