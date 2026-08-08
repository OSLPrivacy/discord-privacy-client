import "./message-timer-picker.css";

/**
 * Mirrors `crates/ipc/src/message_expiry_dial.rs::MessageTimer`: four direct
 * fields, each with its own clock range, converted to one sealed lifetime.
 */
export interface MessageTimerFields {
  days: number;
  hours: number;
  minutes: number;
  seconds: number;
}

export const MESSAGE_TIMER_MAX_DAYS = 30;
export const MESSAGE_TIMER_MAX_SECONDS = MESSAGE_TIMER_MAX_DAYS * 24 * 60 * 60;

const DAY_SECONDS = 24 * 60 * 60;
const HOUR_SECONDS = 60 * 60;
const MINUTE_SECONDS = 60;

export const MESSAGE_TIMER_FIELD_ORDER = ["days", "hours", "minutes", "seconds"] as const;

export function messageTimerTotalSeconds(fields: MessageTimerFields): number {
  return fields.days * DAY_SECONDS
    + fields.hours * HOUR_SECONDS
    + fields.minutes * MINUTE_SECONDS
    + fields.seconds;
}

/**
 * Hours, minutes and seconds use their normal clock ranges so each direct
 * field has one meaning; the total lifetime is the final 30-day boundary.
 */
export function isValidMessageTimerFields(fields: MessageTimerFields): boolean {
  const wholeNonNegative = (value: number): boolean => Number.isInteger(value) && value >= 0;
  return wholeNonNegative(fields.days)
    && wholeNonNegative(fields.hours) && fields.hours < 24
    && wholeNonNegative(fields.minutes) && fields.minutes < 60
    && wholeNonNegative(fields.seconds) && fields.seconds < 60
    && messageTimerTotalSeconds(fields) <= MESSAGE_TIMER_MAX_SECONDS;
}

export function messageTimerDurationWords(fields: MessageTimerFields): string {
  if (!isValidMessageTimerFields(fields)) return "";
  const total = messageTimerTotalSeconds(fields);
  if (total === 0) return "No expiry";
  const parts: string[] = [];
  if (fields.days > 0) parts.push(`${fields.days} day${fields.days === 1 ? "" : "s"}`);
  if (fields.hours > 0) parts.push(`${fields.hours} hour${fields.hours === 1 ? "" : "s"}`);
  if (fields.minutes > 0) parts.push(`${fields.minutes} minute${fields.minutes === 1 ? "" : "s"}`);
  if (fields.seconds > 0) parts.push(`${fields.seconds} second${fields.seconds === 1 ? "" : "s"}`);
  return parts.join(", ");
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

interface FieldSpec {
  key: keyof MessageTimerFields;
  label: string;
  max: number;
}

const FIELD_SPECS: FieldSpec[] = [
  { key: "days", label: "Days", max: MESSAGE_TIMER_MAX_DAYS },
  { key: "hours", label: "Hours", max: 23 },
  { key: "minutes", label: "Minutes", max: 59 },
  { key: "seconds", label: "Seconds", max: 59 },
];

/**
 * Renders the direct timer control. Every field is always present (Days,
 * Hours, Minutes, Seconds) regardless of the current value, so a caller with
 * no chosen expiry still sees the full control. Out-of-range fields are
 * flagged instead of clamped, matching the direct/sealed contract.
 */
export function messageTimerPickerMarkup(fields: MessageTimerFields): string {
  const valid = isValidMessageTimerFields(fields);
  const fieldsMarkup = FIELD_SPECS.map((spec) => {
    const value = fields[spec.key];
    const id = `message-timer-${spec.key}`;
    return `<label class="message-timer-picker-field" for="${id}"><span>${spec.label}</span><input id="${id}" name="message-timer-${spec.key}" type="number" inputmode="numeric" min="0" max="${spec.max}" step="1" value="${Number.isFinite(value) ? value : 0}" aria-describedby="message-timer-result" required></label>`;
  }).join("");
  const result = valid
    ? escapeHtml(messageTimerDurationWords(fields))
    : `<span class="message-timer-picker-error">OSL message expiry must be between 1 second and ${MESSAGE_TIMER_MAX_DAYS} days</span>`;
  return `<fieldset class="message-timer-picker"><legend>Message timer</legend><p>Choose how long this message stays available, up to ${MESSAGE_TIMER_MAX_DAYS} days.</p><div class="message-timer-picker-fields" data-max-days="${MESSAGE_TIMER_MAX_DAYS}">${fieldsMarkup}</div><output id="message-timer-result" class="message-timer-picker-result" aria-live="polite">${result}</output></fieldset>`;
}
