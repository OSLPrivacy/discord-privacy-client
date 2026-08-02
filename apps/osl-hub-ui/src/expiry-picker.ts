import "./expiry-picker.css";

/** Bounds supplied by the sealed-message expiry contract. */
export interface ExpiryBounds {
  minSeconds: number;
  maxSeconds: number;
}

export interface ExpiryPickerModel {
  bounds: ExpiryBounds;
  valueSeconds: number;
  nowMs: number;
}

function validBounds(bounds: ExpiryBounds): boolean {
  return Number.isSafeInteger(bounds.minSeconds)
    && Number.isSafeInteger(bounds.maxSeconds)
    && bounds.minSeconds >= 1
    && bounds.maxSeconds >= bounds.minSeconds;
}

export function isExpirySeconds(value: number, bounds: ExpiryBounds): boolean {
  return validBounds(bounds)
    && Number.isSafeInteger(value)
    && value >= bounds.minSeconds
    && value <= bounds.maxSeconds;
}

export function parseExpirySeconds(value: string, bounds: ExpiryBounds): number | null {
  if (!/^\d+$/u.test(value)) return null;
  const seconds = Number(value);
  return isExpirySeconds(seconds, bounds) ? seconds : null;
}

export function expiryDurationWords(seconds: number): string {
  if (!Number.isSafeInteger(seconds) || seconds < 1) return "";
  const units = [
    [86_400, "day"],
    [3_600, "hour"],
    [60, "minute"],
    [1, "second"],
  ] as const;
  for (const [unitSeconds, label] of units) {
    if (seconds >= unitSeconds && seconds % unitSeconds === 0) {
      const amount = seconds / unitSeconds;
      return `${amount.toLocaleString("en-US")} ${label}${amount === 1 ? "" : "s"}`;
    }
  }
  return `${seconds.toLocaleString("en-US")} seconds`;
}

/** A small non-linear set; exact entry remains available for every permitted value. */
export function expiryPresets(bounds: ExpiryBounds): number[] {
  if (!validBounds(bounds)) return [];
  return [...new Set([bounds.minSeconds, 60, 3_600, 86_400, bounds.maxSeconds])]
    .filter((seconds) => isExpirySeconds(seconds, bounds))
    .sort((left, right) => left - right);
}

export function expiryMomentWords(seconds: number, nowMs: number): string {
  if (!Number.isFinite(nowMs) || !Number.isSafeInteger(seconds) || seconds < 1) return "";
  const moment = new Date(nowMs + seconds * 1_000);
  return `Expires at ${new Intl.DateTimeFormat("en-US", {
    dateStyle: "full",
    timeStyle: "medium",
    timeZone: "UTC",
  }).format(moment)} UTC (${expiryDurationWords(seconds)} after it is opened)`;
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

/**
 * The caller supplies server-contract bounds; this component deliberately has
 * no product ceiling of its own.
 */
export function expiryPickerMarkup(model: ExpiryPickerModel): string {
  if (!isExpirySeconds(model.valueSeconds, model.bounds) || !Number.isFinite(model.nowMs)) return "";
  const presets = expiryPresets(model.bounds)
    .map((seconds) => `<button class="expiry-picker-preset" type="button" data-expiry-seconds="${seconds}" aria-pressed="${seconds === model.valueSeconds}">${escapeHtml(expiryDurationWords(seconds))}</button>`)
    .join("");
  return `<fieldset class="expiry-picker"><legend>Message expiry</legend><p>Choose how long this message stays available after the recipient opens it.</p><div class="expiry-picker-presets" aria-label="Expiry presets">${presets}</div><label class="expiry-picker-exact" for="message-expiry-seconds"><span>Exact seconds</span><input id="message-expiry-seconds" name="message-expiry-seconds" type="number" inputmode="numeric" min="${model.bounds.minSeconds}" max="${model.bounds.maxSeconds}" step="1" value="${model.valueSeconds}" aria-describedby="message-expiry-result" required></label><output id="message-expiry-result" class="expiry-picker-result" aria-live="polite">${escapeHtml(expiryMomentWords(model.valueSeconds, model.nowMs))}</output></fieldset>`;
}
