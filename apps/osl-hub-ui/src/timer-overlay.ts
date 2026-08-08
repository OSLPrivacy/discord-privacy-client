// Greyed-out app overlay for the send-expiry timer picker.
//
// This is the timer screen introduced by TASK 0554. App-specific limits and
// presets live inside this screen so opening a timer never routes to a second,
// competing picker.

export interface TimerOverlayState {
  days: string;
  hours: string;
  minutes: string;
  seconds: string;
}

export type TimerOverlayApp = "Messenger" | "Discord";

export interface TimerOverlayPolicy {
  appName: TimerOverlayApp;
  maxSeconds: number;
  maxLabel: string;
}

export interface TimerOverlayChoice {
  seconds: number;
  label: string;
}

const MINUTE_SECONDS = 60;
const DAY_SECONDS = 24 * 60 * 60;

export const TIMER_OVERLAY_POLICIES: Readonly<Record<TimerOverlayApp, TimerOverlayPolicy>> = {
  Messenger: { appName: "Messenger", maxSeconds: 10 * MINUTE_SECONDS, maxLabel: "10 minutes" },
  Discord: { appName: "Discord", maxSeconds: 30 * DAY_SECONDS, maxLabel: "30 days" },
};

/** Shared choices; the active app policy decides which remain available. */
export const TIMER_OVERLAY_CHOICES: readonly TimerOverlayChoice[] = [
  { seconds: 60, label: "1 minute" },
  { seconds: 5 * MINUTE_SECONDS, label: "5 minutes" },
  { seconds: 10 * MINUTE_SECONDS, label: "10 minutes" },
  { seconds: 60 * MINUTE_SECONDS, label: "1 hour" },
  { seconds: DAY_SECONDS, label: "1 day" },
  { seconds: 7 * DAY_SECONDS, label: "7 days" },
  { seconds: 30 * DAY_SECONDS, label: "30 days" },
];

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

function timerOverlayChoiceMarkup(choice: TimerOverlayChoice, policy: TimerOverlayPolicy): string {
  const unavailable = choice.seconds > policy.maxSeconds;
  const unavailableWords = unavailable
    ? ` title="Longer than ${escapeHtml(policy.appName)}'s ${escapeHtml(policy.maxLabel)} limit"`
    : "";
  return `<button type="button" class="timer-overlay-choice${unavailable ? " timer-overlay-choice-disabled" : ""}" data-timer-choice-seconds="${choice.seconds}" aria-disabled="${unavailable}"${unavailable ? " disabled" : ""}${unavailableWords}>${escapeHtml(choice.label)}</button>`;
}

/**
 * Render TASK 0554's existing timer overlay for the app being composed in.
 * Discord remains the default for callers of the original one-argument API.
 */
export function timerOverlayMarkup(
  appOrState: TimerOverlayApp | TimerOverlayState = "Discord",
  suppliedState: TimerOverlayState = defaultTimerOverlayState(),
): string {
  const appName = typeof appOrState === "string" ? appOrState : "Discord";
  const state = typeof appOrState === "string" ? suppliedState : appOrState;
  const policy = TIMER_OVERLAY_POLICIES[appName];
  const fields = TIMER_OVERLAY_FIELDS.map((field) => timerOverlayFieldMarkup(field, state)).join("\n");
  const choices = TIMER_OVERLAY_CHOICES.map((choice) => timerOverlayChoiceMarkup(choice, policy)).join("\n");
  return `
    <div class="timer-overlay" role="dialog" aria-modal="true" aria-labelledby="timer-overlay-heading" data-timer-app="${policy.appName}" data-timer-max-seconds="${policy.maxSeconds}">
      <div class="timer-overlay-scrim"></div>
      <div class="timer-overlay-panel">
        <h2 id="timer-overlay-heading" class="timer-overlay-heading">Set a timer</h2>
        <p class="timer-overlay-limit"><strong>${escapeHtml(policy.appName)}</strong> supports timers up to <strong>${escapeHtml(policy.maxLabel)}</strong>.</p>
        <div class="timer-overlay-choices" aria-label="Timer choices">${choices}</div>
        <div class="timer-overlay-fields">${fields}</div>
        <output id="timer-overlay-error" class="timer-overlay-error" aria-live="polite"></output>
        <button type="button" id="timer-overlay-save" class="timer-overlay-save">Save</button>
      </div>
    </div>`;
}
