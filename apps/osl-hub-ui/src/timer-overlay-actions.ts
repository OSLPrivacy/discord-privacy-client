// Connects the timer overlay picker (timer-overlay.ts) to the send timer
// value the message is actually sent with. Mirrors the validation in
// apps/osl-hub/src/security.rs (timer_picker_state / TIMER_PICKER_MAX_DAYS =
// 30 days, i.e. day 31 is rejected) so the UI and the backend agree on what
// a "valid" timer picker save looks like before any IPC round trip exists.

import { defaultTimerOverlayState, type TimerOverlayState } from "./timer-overlay";

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

/** Pure validation + duration calculation, mirroring security.rs::timer_picker_duration_seconds. */
export function timerOverlaySaveDurationSeconds(state: TimerOverlayState): TimerOverlaySaveResult {
  try {
    const days = parseTimerOverlayPart(state.days, "days", TIMER_OVERLAY_MAX_DAYS);
    const hours = parseTimerOverlayPart(state.hours, "hours", 23);
    const minutes = parseTimerOverlayPart(state.minutes, "minutes", 59);
    const seconds = parseTimerOverlayPart(state.seconds, "seconds", 59);
    const durationSeconds = days * 86_400 + hours * 3_600 + minutes * 60 + seconds;
    if (durationSeconds === 0) {
      return { ok: false, error: "OSL timer picker duration must be at least 01 second" };
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
  save(): TimerOverlaySaveResult;
  subscribe(listener: () => void): () => void;
}

/**
 * Holds the live picker state plus, once `save()` succeeds, the send timer
 * value (duration in seconds) that a send command should use. A failed
 * `save()` clears the send value and records the validation message instead,
 * so the overlay can show it without a stale value lingering.
 */
export function createTimerOverlaySaveActions(
  initialState: TimerOverlayState = defaultTimerOverlayState(),
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
    save() {
      const result = timerOverlaySaveDurationSeconds(state);
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

/**
 * Wires timer-overlay.ts's rendered fields and Save button
 * (`#timer-overlay-save`, `#timer-overlay-error`) to a
 * `TimerOverlaySaveActions` controller: field input reflects into the
 * controller's state, and clicking Save runs validation and shows the
 * rejection message (e.g. day 31) instead of silently doing nothing.
 */
export function bindTimerOverlaySave(root: ParentNode, actions: TimerOverlaySaveActions): void {
  for (const key of ["days", "hours", "minutes", "seconds"] as const) {
    const input = root.querySelector<HTMLInputElement>(`#timer-overlay-${key}`);
    input?.addEventListener("input", () => {
      actions.setField(key, input.value);
    });
  }
  const saveButton = root.querySelector<HTMLButtonElement>("#timer-overlay-save");
  const errorOutput = root.querySelector<HTMLOutputElement>("#timer-overlay-error");
  saveButton?.addEventListener("click", () => {
    const result = actions.save();
    if (errorOutput) {
      errorOutput.textContent = result.ok ? "" : result.error;
    }
  });
}
