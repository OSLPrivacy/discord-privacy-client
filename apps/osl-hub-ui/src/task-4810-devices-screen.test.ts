/**
 * TASK 4810 - the erase request is a request, and the screen never pretends
 * otherwise.
 *
 * Finish line, item by item:
 *
 *   1. a removed device that stays offline shows the exact two lines
 *      "Cut off" and "Not acknowledged" across 24 simulated hours, with 0
 *      changes to either line;
 *   2. bringing it online and letting it erase changes the second line to
 *      "Erased" with the date;
 *   3. a search of the devices screen source finds 0 uses of the words wiped,
 *      deleted or gone applied to a device that has not acknowledged;
 *   4. the screen carries one sentence saying a device that never comes back
 *      never erases.
 *
 * The 24-hour run is a real sweep, not an assertion about one sample: the
 * screen is rebuilt once a simulated minute for 1440 minutes and every sample
 * is compared against the previous one, so an implementation that flipped the
 * erase line after any threshold inside a day would be counted flipping. The
 * device is offline for the whole sweep, which in this model means exactly one
 * thing: no `DeviceEraseReport` ever arrives.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  CUT_OFF_LINE,
  NEVER_COMES_BACK_SENTENCE,
  NOT_ACKNOWLEDGED_LINE,
  acceptEraseReport,
  deviceRemovalLines,
  devicesScreenMarkup,
  removeDevice,
  type ActiveDevice,
  type DeviceRemovalLine,
  type DevicesScreenModel,
  type RemovedDevice,
} from "./devices-screen";

const SOURCE_URL = new URL("./devices-screen.ts", import.meta.url);
const SOURCE = readFileSync(SOURCE_URL, "utf8");

const LAPTOP: ActiveDevice = { deviceId: "device-2", displayName: "Old laptop" };
const PHONE: ActiveDevice = { deviceId: "device-1", displayName: "Phone" };

const REMOVED_AT = "2026-08-07T09:00:00.000Z";
const REMOVED_AT_MS = Date.parse(REMOVED_AT);
const MINUTE_MS = 60_000;
const MINUTES_IN_24_HOURS = 24 * 60;

/** The words a person actually reads, banned unless the device has answered. */
const FORBIDDEN_WORDS = /\b(wiped|deleted|gone)\b/giu;

function model(removed: readonly RemovedDevice[]): DevicesScreenModel {
  return { activeDevices: [PHONE], removedDevices: removed };
}

function lineKey(line: DeviceRemovalLine): string {
  return JSON.stringify([line.effect, line.text, line.caption, line.settled, line.tone]);
}

function countOccurrences(haystack: string, needle: string): number {
  let count = 0;
  let from = 0;
  for (;;) {
    const at = haystack.indexOf(needle, from);
    if (at < 0) return count;
    count += 1;
    from = at + needle.length;
  }
}

/** Plain text of the markup: what a person reads, with the tags taken out. */
function readableText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("TASK 4810 devices screen", () => {
  it("shows Cut off and Not acknowledged unchanged across 24 simulated hours offline", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);

    let samples = 0;
    let cutOffChanges = 0;
    let erasedChanges = 0;
    let markupChanges = 0;
    let previousCutOff: string | null = null;
    let previousErased: string | null = null;
    let previousMarkup: string | null = null;
    const cutOffTexts = new Set<string>();
    const erasedTexts = new Set<string>();

    for (let minute = 0; minute <= MINUTES_IN_24_HOURS; minute += 1) {
      const now = new Date(REMOVED_AT_MS + minute * MINUTE_MS).toISOString();
      const [cutOff, erased] = deviceRemovalLines(offline, now);
      const markup = devicesScreenMarkup(model([offline]), now);
      samples += 1;
      cutOffTexts.add(cutOff.text);
      erasedTexts.add(erased.text);
      if (previousCutOff !== null && lineKey(cutOff) !== previousCutOff) cutOffChanges += 1;
      if (previousErased !== null && lineKey(erased) !== previousErased) erasedChanges += 1;
      if (previousMarkup !== null && markup !== previousMarkup) markupChanges += 1;
      previousCutOff = lineKey(cutOff);
      previousErased = lineKey(erased);
      previousMarkup = markup;
    }

    // eslint-disable-next-line no-console
    console.log(
      `TASK4810_OFFLINE simulated_hours=24 samples=${samples} line1_changes=${cutOffChanges} line2_changes=${erasedChanges} markup_changes=${markupChanges} line1="${[...cutOffTexts].join("|")}" line2="${[...erasedTexts].join("|")}"`,
    );

    expect(samples).toBe(MINUTES_IN_24_HOURS + 1);
    expect(cutOffChanges).toBe(0);
    expect(erasedChanges).toBe(0);
    expect(markupChanges).toBe(0);
    expect([...cutOffTexts]).toEqual([CUT_OFF_LINE]);
    expect([...erasedTexts]).toEqual([NOT_ACKNOWLEDGED_LINE]);
    expect([...cutOffTexts]).toEqual(["Cut off"]);
    expect([...erasedTexts]).toEqual(["Not acknowledged"]);
  });

  it("is still Not acknowledged a year later, because silence is not an answer", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    const aYearOn = new Date(REMOVED_AT_MS + 365 * 24 * 60 * MINUTE_MS).toISOString();
    const [cutOff, erased] = deviceRemovalLines(offline, aYearOn);
    // eslint-disable-next-line no-console
    console.log(`TASK4810_SILENCE days=365 line1="${cutOff.text}" line2="${erased.text}" settled=${erased.settled}`);
    expect(cutOff.text).toBe("Cut off");
    expect(erased.text).toBe("Not acknowledged");
    expect(erased.settled).toBe(false);
    expect(erased.tone).toBe("unconfirmed");
  });

  it("draws no pending animation and reads no ambient clock", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    const markup = devicesScreenMarkup(model([offline]), REMOVED_AT);
    const machinery = ["Date.now(", "setTimeout(", "setInterval(", "requestAnimationFrame", "spinner", "aria-busy"];
    const found = machinery.filter((token) => SOURCE.includes(token));
    const inMarkup = ["spinner", "aria-busy", "progress", "pending"].filter((token) => markup.includes(token));
    // eslint-disable-next-line no-console
    console.log(`TASK4810_NO_TIMER source_timer_tokens=${found.length} markup_pending_tokens=${inMarkup.length}`);
    expect(found).toEqual([]);
    expect(inMarkup).toEqual([]);
  });

  it("changes the second line to Erased with the date only when the device answers", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    const before = deviceRemovalLines(offline, "2026-08-08T09:00:00.000Z")[1];
    expect(before.text).toBe("Not acknowledged");

    const answered = acceptEraseReport(offline, {
      deviceId: "device-2",
      source: "device",
      erasedAt: "2026-08-08T18:42:00.000Z",
    });
    const [cutOff, erased] = deviceRemovalLines(answered, "2026-08-08T18:42:30.000Z");
    const markup = devicesScreenMarkup(model([answered]), "2026-08-08T18:42:30.000Z");

    // eslint-disable-next-line no-console
    console.log(
      `TASK4810_ANSWERED line1="${cutOff.text}" line2_before="${before.text}" line2_after="${erased.text}" settled=${erased.settled} acknowledged_attr=${/data-erase-acknowledged="true"/u.test(markup)}`,
    );

    expect(cutOff.text).toBe("Cut off");
    expect(erased.text).toBe("Erased 8 August 2026");
    expect(erased.text.startsWith("Erased ")).toBe(true);
    expect(erased.settled).toBe(true);
    expect(erased.tone).toBe("done");
    expect(markup).toContain("Erased 8 August 2026");
    expect(markup).toContain('data-erase-acknowledged="true"');
  });

  it("refuses an erase answer OSL wrote on the device's behalf", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    expect(() =>
      acceptEraseReport(offline, {
        deviceId: "device-2",
        source: "hub" as unknown as "device",
        erasedAt: "2026-08-08T18:42:00.000Z",
      }),
    ).toThrow(/only the device itself can answer/u);
    expect(() =>
      acceptEraseReport(offline, { deviceId: "device-9", source: "device", erasedAt: "2026-08-08T18:42:00.000Z" }),
    ).toThrow(/names "device-9"/u);
    expect(deviceRemovalLines(offline, "2026-08-09T00:00:00.000Z")[1].text).toBe("Not acknowledged");
  });

  it("uses none of the words wiped, deleted or gone about a device that has not acknowledged", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    const answered = acceptEraseReport(offline, {
      deviceId: "device-2",
      source: "device",
      erasedAt: "2026-08-08T18:42:00.000Z",
    });
    const offlineMarkup = devicesScreenMarkup(model([offline]), "2026-08-08T09:00:00.000Z");
    const answeredMarkup = devicesScreenMarkup(model([answered]), "2026-08-08T18:42:30.000Z");

    const sourceHits = SOURCE.match(FORBIDDEN_WORDS) ?? [];
    const offlineHits = readableText(offlineMarkup).match(FORBIDDEN_WORDS) ?? [];
    const answeredHits = readableText(answeredMarkup).match(FORBIDDEN_WORDS) ?? [];

    // eslint-disable-next-line no-console
    console.log(
      `TASK4810_WORDS source_file=devices-screen.ts source_hits=${sourceHits.length} unacknowledged_screen_hits=${offlineHits.length} acknowledged_screen_hits=${answeredHits.length} searched="wiped,deleted,gone"`,
    );

    expect(sourceHits).toEqual([]);
    expect(offlineHits).toEqual([]);
    expect(answeredHits).toEqual([]);
  });

  it("carries one sentence saying a device that never comes back never erases", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    const markup = devicesScreenMarkup(model([offline]), "2026-08-08T09:00:00.000Z");
    const occurrences = countOccurrences(readableText(markup), NEVER_COMES_BACK_SENTENCE);
    // eslint-disable-next-line no-console
    console.log(`TASK4810_SENTENCE occurrences=${occurrences} sentence="${NEVER_COMES_BACK_SENTENCE}"`);
    expect(NEVER_COMES_BACK_SENTENCE).toBe("A device that never comes back online never erases what it holds.");
    expect(occurrences).toBe(1);
  });

  it("refuses to draw a removal record that contradicts itself", () => {
    const offline = removeDevice(LAPTOP, REMOVED_AT);
    expect(() => deviceRemovalLines(offline, "2026-08-06T09:00:00.000Z")).toThrow(/cut off in the future/u);
    expect(() => deviceRemovalLines({ ...offline, displayName: "  " }, REMOVED_AT)).toThrow(/has no name/u);
  });
});
