// Connects TASK 0554's timer overlay to the send timer value. The active app's
// visible policy is also enforced here, so hand-editing the four fields cannot
// bypass a greyed-out preset.

import {
  defaultTimerOverlayState,
  TIMER_OVERLAY_POLICIES,
  type TimerOverlayApp,
  type TimerOverlayState,
} from "./timer-overlay";

export const TIMER_OVERLAY_MAX_DAYS = 30;

export type TimerOverlaySaveResult =
  | { ok: true; durationSeconds: number }
  | { ok: false; error: string };

function parseTimerOverlayPart(raw: string, label: string, maximum: number): number {
  if (!/^\d{2}$/u.test(raw)) {
    throw new Error(`OSL timer picker ${label} must be two digits`);
  }
  const value = Number(raw);
  if (value > maximum) {
    throw new Error(`OSL timer picker ${label} must be between 00 and ${maximum.toString().padStart(2, "0")}`);
  }
  return value;
}

export function timerOverlayStateForDuration(durationSeconds: number): TimerOverlayState {
  const days = Math.floor(durationSeconds / 86_400);
  const afterDays = durationSeconds % 86_400;
  const hours = Math.floor(afterDays / 3_600);
  const afterHours = afterDays % 3_600;
  const minutes = Math.floor(afterHours / 60);
  const seconds = afterHours % 60;
  const twoDigits = (value: number): string => value.toString().padStart(2, "0");
  return {
    days: twoDigits(days),
    hours: twoDigits(hours),
    minutes: twoDigits(minutes),
    seconds: twoDigits(seconds),
  };
}

/** Pure validation plus app-specific duration admission. */
export function timerOverlaySaveDurationSeconds(
  state: TimerOverlayState,
  appName: TimerOverlayApp = "Discord",
): TimerOverlaySaveResult {
  try {
    const days = parseTimerOverlayPart(state.days, "days", TIMER_OVERLAY_MAX_DAYS);
    const hours = parseTimerOverlayPart(state.hours, "hours", 23);
    const minutes = parseTimerOverlayPart(state.minutes, "minutes", 59);
    const seconds = parseTimerOverlayPart(state.seconds, "seconds", 59);
    const durationSeconds = days * 86_400 + hours * 3_600 + minutes * 60 + seconds;
    if (durationSeconds === 0) {
      return { ok: false, error: "OSL timer picker duration must be at least 01 second" };
    }
    const policy = TIMER_OVERLAY_POLICIES[appName];
    if (durationSeconds > policy.maxSeconds) {
      return {
        ok: false,
        error: `OSL: ${policy.appName} cannot keep that timer; longest supported timer is ${policy.maxLabel}`,
      };
    }
    return { ok: true, durationSeconds };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

export interface TimerOverlaySaveActions {
  getState(): TimerOverlayState;
  getSendTimerValueSeconds(): number | null;
  getValidationError(): string | null;
  setField(key: keyof TimerOverlayState, raw: string): void;
  choose(durationSeconds: number): boolean;
  save(): TimerOverlaySaveResult;
  subscribe(listener: () => void): () => void;
}

export function createTimerOverlaySaveActions(
  initialState: TimerOverlayState = defaultTimerOverlayState(),
  appName: TimerOverlayApp = "Discord",
): TimerOverlaySaveActions {
  let state = initialState;
  let sendTimerValueSeconds: number | null = null;
  let validationError: string | null = null;
  const listeners = new Set<() => void>();
  const notify = () => {
    for (const listener of listeners) listener();
  };

  return {
    getState: () => state,
    getSendTimerValueSeconds: () => sendTimerValueSeconds,
    getValidationError: () => validationError,
    setField(key, raw) {
      state = { ...state, [key]: raw };
      notify();
    },
    choose(durationSeconds) {
      if (durationSeconds <= 0 || durationSeconds > TIMER_OVERLAY_POLICIES[appName].maxSeconds) return false;
      state = timerOverlayStateForDuration(durationSeconds);
      validationError = null;
      notify();
      return true;
    },
    save() {
      const result = timerOverlaySaveDurationSeconds(state, appName);
      if (result.ok) {
        sendTimerValueSeconds = result.durationSeconds;
        validationError = null;
      } else {
        sendTimerValueSeconds = null;
        validationError = result.error;
      }
      notify();
      return result;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

export function bindTimerOverlaySave(root: ParentNode, actions: TimerOverlaySaveActions): void {
  for (const key of ["days", "hours", "minutes", "seconds"] as const) {
    const input = root.querySelector<HTMLInputElement>(`#timer-overlay-${key}`);
    input?.addEventListener("input", () => actions.setField(key, input.value));
  }
  for (const choice of root.querySelectorAll<HTMLButtonElement>("[data-timer-choice-seconds]:not(:disabled)")) {
    choice.addEventListener("click", () => {
      const durationSeconds = Number(choice.dataset.timerChoiceSeconds);
      if (!actions.choose(durationSeconds)) return;
      const state = actions.getState();
      for (const key of ["days", "hours", "minutes", "seconds"] as const) {
        const input = root.querySelector<HTMLInputElement>(`#timer-overlay-${key}`);
        if (input) input.value = state[key];
      }
    });
  }
  const saveButton = root.querySelector<HTMLButtonElement>("#timer-overlay-save");
  const errorOutput = root.querySelector<HTMLOutputElement>("#timer-overlay-error");
  saveButton?.addEventListener("click", () => {
    const result = actions.save();
    if (errorOutput) errorOutput.textContent = result.ok ? "" : result.error;
  });
}
