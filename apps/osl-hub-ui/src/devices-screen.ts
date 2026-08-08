/**
 * TASK 4810 - the erase request is a request, and the screen never pretends
 * otherwise.
 *
 * Gate 4809 made removal real on the side OSL controls: a removed device stops
 * being addressed, gets no slot in anything sent afterwards, and fetches
 * nothing new. That half is ours, it is local, and it is already true the
 * moment the person presses Remove.
 *
 * The other half is not ours. Removing a device also ASKS it to erase what it
 * already holds, and that ask travels to a machine we do not control. A laptop
 * that is switched off never receives it, never runs it, and never answers.
 * There is no deadline after which "it probably ran" becomes a fact, so this
 * screen never draws one.
 *
 * So the two effects are two separate lines:
 *
 *   line 1  "Cut off"           - already happened, ours, settled;
 *   line 2  "Not acknowledged"  - a request with no answer yet,
 *           or "Erased <date>"  - only after the device itself says so.
 *
 * Three rules this module exists to keep:
 *
 *   1. `deviceRemovalLines` takes the current instant and never consults it for
 *      the erase verdict. Redrawing the screen every second for a year moves
 *      neither line. The only thing that can move line 2 is
 *      `acceptEraseReport`, which requires the device's own answer.
 *   2. There is no in-progress animation on the erase line. Such an animation
 *      promises that something is under way and will finish; against a
 *      switched-off laptop both halves of that promise are false.
 *   3. The words on this screen never say the held copy is unrecoverable while
 *      the device has not answered. "Not acknowledged" means not acknowledged.
 *
 * The wording of line 2 is deliberately the same as the burn dialog's
 * (`burn-revocation-receipt.ts`, `Not acknowledged`): the same distinction
 * between a request sent and a request carried out, spelled the same way in
 * both places.
 */

/** Line 1, always. The half OSL performed itself. */
export const CUT_OFF_LINE = "Cut off";

/** Line 2 until the device answers. Never an animation, never a tick. */
export const NOT_ACKNOWLEDGED_LINE = "Not acknowledged";

/** The one sentence this screen owes the person, stated once. */
export const NEVER_COMES_BACK_SENTENCE =
  "A device that never comes back online never erases what it holds.";

/** What removal actually does, said before it is done. */
export const REMOVAL_EXPLANATION =
  "Removing a device cuts it off here and asks it to erase what it holds. Cutting off is ours to do. Erasing is the device's own work, and only the device can say it happened.";

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

/** A device still paired to this account. */
export interface ActiveDevice {
  readonly deviceId: string;
  readonly displayName: string;
}

/**
 * The device's own answer. Nothing else may build one of these: OSL cannot
 * observe an erase it did not witness, so the only honest source is the device.
 */
export interface DeviceEraseReport {
  readonly deviceId: string;
  /** The single accepted origin. A locally invented answer is refused. */
  readonly source: "device";
  /** When the device says it finished, ISO 8601. */
  readonly erasedAt: string;
}

/** A device the person removed. */
export interface RemovedDevice {
  readonly deviceId: string;
  readonly displayName: string;
  /** When OSL stopped addressing it. Local, already true. ISO 8601. */
  readonly cutOffAt: string;
  /** The device's answer, or null while it has not answered. */
  readonly eraseReport: DeviceEraseReport | null;
}

export type DeviceRemovalEffect = "cutOff" | "erased";

/** One of the two lines. `text` is exactly the words a person reads. */
export interface DeviceRemovalLine {
  readonly effect: DeviceRemovalEffect;
  readonly text: string;
  /** Supporting words under the line. Never contradicts or softens `text`. */
  readonly caption: string;
  /** True only when the fact behind the line is established. */
  readonly settled: boolean;
  readonly tone: "done" | "unconfirmed";
}

export interface DevicesScreenModel {
  readonly activeDevices: readonly ActiveDevice[];
  readonly removedDevices: readonly RemovedDevice[];
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function instant(value: unknown): number | null {
  if (typeof value !== "string" || value.trim() === "") return null;
  const parsed = new Date(value).getTime();
  return Number.isFinite(parsed) ? parsed : null;
}

/** "8 August 2026". Built from UTC parts so the same answer reads the same everywhere. */
export function erasedDayLabel(iso: string): string {
  const parsed = new Date(iso);
  const stamp = parsed.getTime();
  if (!Number.isFinite(stamp)) throw new Error(`OSL: erase answer has no usable time: ${JSON.stringify(iso)}`);
  return `${parsed.getUTCDate()} ${MONTHS[parsed.getUTCMonth()]} ${parsed.getUTCFullYear()}`;
}

/** Turn a paired device into a removed one. Cut off now; nothing claimed about the copy it holds. */
export function removeDevice(device: ActiveDevice, cutOffAt: string): RemovedDevice {
  if (instant(cutOffAt) === null) {
    throw new Error(`OSL: cannot record a removal without a time: ${JSON.stringify(cutOffAt)}`);
  }
  return {
    deviceId: device.deviceId,
    displayName: device.displayName,
    cutOffAt,
    eraseReport: null,
  };
}

/**
 * The ONLY way line 2 ever changes. The answer has to name this device and has
 * to come from the device; anything else is OSL guessing on the device's behalf,
 * which is the whole thing this screen refuses to do.
 */
export function acceptEraseReport(device: RemovedDevice, report: DeviceEraseReport): RemovedDevice {
  if (report.source !== "device") {
    throw new Error(`OSL: only the device itself can answer an erase request, not ${JSON.stringify(report.source)}`);
  }
  if (report.deviceId !== device.deviceId) {
    throw new Error(`OSL: erase answer names ${JSON.stringify(report.deviceId)}, not ${JSON.stringify(device.deviceId)}`);
  }
  const erased = instant(report.erasedAt);
  const cutOff = instant(device.cutOffAt);
  if (erased === null) {
    throw new Error(`OSL: erase answer has no usable time: ${JSON.stringify(report.erasedAt)}`);
  }
  if (cutOff !== null && erased < cutOff) {
    throw new Error("OSL: erase answer is older than the removal it answers");
  }
  return { ...device, eraseReport: report };
}

/** Every way a removed device's record contradicts itself. Empty means drawable. */
export function removedDeviceErrors(device: RemovedDevice, now: string): string[] {
  const errors: string[] = [];
  if (!device.deviceId?.trim()) errors.push("removed device has no id");
  if (!device.displayName?.trim()) errors.push(`device ${device.deviceId} has no name`);
  const cutOff = instant(device.cutOffAt);
  const present = instant(now);
  if (cutOff === null) errors.push(`device ${device.deviceId} has no cut-off time`);
  if (present === null) errors.push(`the current time is not usable: ${JSON.stringify(now)}`);
  if (cutOff !== null && present !== null && cutOff > present) {
    errors.push(`device ${device.deviceId} was cut off in the future`);
  }
  const report = device.eraseReport;
  if (report) {
    if (report.source !== "device") errors.push(`device ${device.deviceId} has an erase answer OSL wrote for it`);
    if (report.deviceId !== device.deviceId) errors.push(`device ${device.deviceId} holds an answer from ${report.deviceId}`);
    if (instant(report.erasedAt) === null) errors.push(`device ${device.deviceId} has an erase answer with no usable time`);
  }
  return errors;
}

/**
 * The two lines, in order.
 *
 * `now` is taken because a screen gets redrawn on a tick, and refused if it
 * contradicts the record. It is NEVER consulted for the erase verdict: no
 * elapsed time, no threshold, no "it has been long enough". Line 2 reads the
 * device's answer or it reads `Not acknowledged`.
 */
export function deviceRemovalLines(
  device: RemovedDevice,
  now: string,
): readonly [DeviceRemovalLine, DeviceRemovalLine] {
  const errors = removedDeviceErrors(device, now);
  if (errors.length > 0) {
    throw new Error(`OSL: this removed device cannot be drawn honestly: ${errors.join("; ")}`);
  }
  const cutOff: DeviceRemovalLine = {
    effect: "cutOff",
    text: CUT_OFF_LINE,
    caption: "This already happened. Nothing new reaches this device.",
    settled: true,
    tone: "done",
  };
  const report = device.eraseReport;
  if (!report) {
    return [
      cutOff,
      {
        effect: "erased",
        text: NOT_ACKNOWLEDGED_LINE,
        caption: "OSL asked this device to erase what it holds. It has not answered.",
        settled: false,
        tone: "unconfirmed",
      },
    ];
  }
  return [
    cutOff,
    {
      effect: "erased",
      text: `Erased ${erasedDayLabel(report.erasedAt)}`,
      caption: "This device answered, and said it erased what it held.",
      settled: true,
      tone: "done",
    },
  ];
}

function lineMarkup(line: DeviceRemovalLine): string {
  return `<li class="device-removal-line" data-removal-effect="${line.effect}" data-settled="${line.settled}" data-tone="${line.tone}">
      <strong class="device-removal-line-text" data-removal-line="${line.effect}">${escapeHtml(line.text)}</strong>
      <small class="device-removal-line-caption">${escapeHtml(line.caption)}</small>
    </li>`;
}

function removedDeviceMarkup(device: RemovedDevice, now: string): string {
  const [cutOff, erased] = deviceRemovalLines(device, now);
  return `<article class="device-row removed-device" data-removed-device="${escapeHtml(device.deviceId)}" data-erase-acknowledged="${erased.settled}">
    <h3 class="device-row-name">${escapeHtml(device.displayName)}</h3>
    <ul class="device-removal-lines">
      ${lineMarkup(cutOff)}
      ${lineMarkup(erased)}
    </ul>
  </article>`;
}

function activeDeviceMarkup(device: ActiveDevice): string {
  return `<article class="device-row active-device" data-active-device="${escapeHtml(device.deviceId)}">
    <h3 class="device-row-name">${escapeHtml(device.displayName)}</h3>
    <button class="button compact" type="button" data-remove-device="${escapeHtml(device.deviceId)}">Remove</button>
  </article>`;
}

/**
 * The whole devices screen. Refuses to draw a device whose record contradicts
 * itself rather than drawing it calmly.
 */
export function devicesScreenMarkup(model: DevicesScreenModel, now: string): string {
  const active = model.activeDevices.length
    ? model.activeDevices.map((device) => activeDeviceMarkup(device)).join("")
    : `<div class="empty-state"><strong>No other devices</strong><p>Only this device is paired to your account.</p></div>`;
  const removed = model.removedDevices.length
    ? model.removedDevices.map((device) => removedDeviceMarkup(device, now)).join("")
    : `<div class="empty-state"><strong>No removed devices</strong><p>Nothing has been removed from your account.</p></div>`;
  return `<section class="devices-screen" data-devices-screen="task-4810" aria-labelledby="devices-screen-title">
    <header class="devices-screen-head">
      <h2 class="devices-screen-title" id="devices-screen-title" tabindex="-1">Devices</h2>
      <p class="devices-screen-explanation">${escapeHtml(REMOVAL_EXPLANATION)}</p>
      <p class="devices-screen-limit" data-devices-limit>${escapeHtml(NEVER_COMES_BACK_SENTENCE)}</p>
    </header>
    <section class="devices-screen-group" data-devices-group="active" aria-labelledby="devices-active-title">
      <h3 id="devices-active-title">Your devices</h3>
      ${active}
    </section>
    <section class="devices-screen-group" data-devices-group="removed" aria-labelledby="devices-removed-title">
      <h3 id="devices-removed-title">Removed</h3>
      ${removed}
    </section>
  </section>`;
}
