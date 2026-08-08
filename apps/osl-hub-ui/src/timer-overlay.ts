// Greyed-out app overlay for the send-expiry timer picker
// (apps/osl-hub/src/security.rs::default_timer_picker_state /
// timer_picker_state). The overlay dims the app behind it and surfaces the
// four two-digit fields the backend DTO carries, in the order it carries
// them: Days, Hours, Minutes, Seconds.

export interface TimerOverlayState {
  days: string;
  hours: string;
  minutes: string;
  seconds: string;
}

// Mirrors security.rs::default_timer_picker_state(): every field starts at
// the two-digit string "00".
export function defaultTimerOverlayState(): TimerOverlayState {
  return { days: "00", hours: "00", minutes: "00", seconds: "00" };
}

interface TimerOverlayField {
  key: keyof TimerOverlayState;
  label: string;
}

export const TIMER_OVERLAY_FIELDS: readonly TimerOverlayField[] = [
  { key: "days", label: "Days" },
  { key: "hours", label: "Hours" },
  { key: "minutes", label: "Minutes" },
  { key: "seconds", label: "Seconds" },
];

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

export function timerOverlayFieldMarkup(field: TimerOverlayField, state: TimerOverlayState): string {
  const value = escapeHtml(state[field.key]);
  return `
    <label class="timer-overlay-field" for="timer-overlay-${field.key}">
      <span class="timer-overlay-field-label">${escapeHtml(field.label)}</span>
      <input
        id="timer-overlay-${field.key}"
        class="timer-overlay-field-input"
        name="timer-overlay-${field.key}"
        type="text"
        inputmode="numeric"
        pattern="[0-9]{2}"
        maxlength="2"
        value="${value}"
        aria-label="${escapeHtml(field.label)}"
      />
    </label>`;
}

export function timerOverlayMarkup(state: TimerOverlayState = defaultTimerOverlayState()): string {
  const fields = TIMER_OVERLAY_FIELDS.map((field) => timerOverlayFieldMarkup(field, state)).join("\n");
  return `
    <div class="timer-overlay" role="dialog" aria-modal="true" aria-labelledby="timer-overlay-heading">
      <div class="timer-overlay-scrim"></div>
      <div class="timer-overlay-panel">
        <h2 id="timer-overlay-heading" class="timer-overlay-heading">Set a timer</h2>
        <div class="timer-overlay-fields">${fields}</div>
        <output id="timer-overlay-error" class="timer-overlay-error" aria-live="polite"></output>
        <button type="button" id="timer-overlay-save" class="timer-overlay-save">Save</button>
      </div>
    </div>`;
}
