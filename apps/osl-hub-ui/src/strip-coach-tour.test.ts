import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  STRIP_COACH_STEPS,
  dismissStripCoach,
  placeStripCoachBubble,
  stripCoachWasDismissed,
} from "./strip-coach-tour";

function source(): string {
  return readFileSync(fileURLToPath(new URL("./strip-coach-tour.ts", import.meta.url)), "utf8");
}

function overlaySource(): string {
  return readFileSync(fileURLToPath(new URL("./overlay.ts", import.meta.url)), "utf8");
}

function inside(x: number, y: number, rect: { left: number; top: number; width: number; height: number }): boolean {
  return x >= rect.left && x <= rect.left + rect.width && y >= rect.top && y <= rect.top + rect.height;
}

describe("Strip first-use coach tour", () => {
  it("contains exactly the owner-ordered nine-tip set and derives its counter from it", () => {
    expect(STRIP_COACH_STEPS.map((step) => step.title)).toEqual([
      "LOCK",
      "COMPOSER",
      "REVEAL",
      "VIEW ONCE",
      "TIMER",
      "BURN",
      "VERIFIED SENDERS",
      "YOUR PLAN",
      "QUICK SETTINGS",
    ]);
    expect(STRIP_COACH_STEPS).toHaveLength(9);
    expect(source()).toContain("TIP ${index + 1} OF ${STRIP_COACH_STEPS.length}");
  });

  it("uses owner-approved click-toggle reveal copy and never puts a hold gesture in a tip", () => {
    const reveal = STRIP_COACH_STEPS.find((step) => step.id === "reveal");
    expect(reveal?.body).toContain("click toggle");
    for (const step of STRIP_COACH_STEPS) expect(step.body).not.toMatch(/\bhold\b|press and hold/iu);
  });

  it("puts every arrow tip inside its measured target at three window sizes", () => {
    for (const { viewport, target } of [
      { viewport: { width: 1440, height: 900 }, target: { left: 640, top: 514, width: 42, height: 30 } },
      { viewport: { width: 1024, height: 768 }, target: { left: 440, top: 460, width: 42, height: 30 } },
      { viewport: { width: 480, height: 720 }, target: { left: 280, top: 430, width: 42, height: 30 } },
    ]) {
      const placement = placeStripCoachBubble(target, viewport);
      expect(inside(placement.arrowTipX, placement.arrowTipY, target)).toBe(true);
    }
  });

  it("measures a moved control again instead of retaining a fixed bubble offset", () => {
    const viewport = { width: 1280, height: 800 };
    const before = placeStripCoachBubble({ left: 110, top: 120, width: 36, height: 28 }, viewport);
    const after = placeStripCoachBubble({ left: 790, top: 590, width: 36, height: 28 }, viewport);
    expect([after.x, after.y, after.arrowTipX, after.arrowTipY]).not.toEqual([before.x, before.y, before.arrowTipX, before.arrowTipY]);
    expect(inside(after.arrowTipX, after.arrowTipY, { left: 790, top: 590, width: 36, height: 28 })).toBe(true);
  });

  it("places the composer card above the actual input and points its arrow down into it", () => {
    const target = { left: 255, top: 630, width: 500, height: 42 };
    const placement = placeStripCoachBubble(target, { width: 1100, height: 820 }, true);
    expect(placement.y + placement.height).toBeLessThanOrEqual(target.top);
    expect(placement.arrowTipY).toBeGreaterThan(target.top);
    expect(inside(placement.arrowTipX, placement.arrowTipY, target)).toBe(true);
  });

  it("persists Skip and finish per carrier and opens quick settings only from the final tip", () => {
    const text = source();
    expect(text).toContain('osl-strip-coach-dismissed-v1:${carrierId}');
    expect(text.match(/persistDismissal\(\);/gu)).toHaveLength(2);
    expect(text).toContain("options.onFinishOpenQuickSettings();");
    expect(overlaySource()).toContain('carrierId: "discord"');
    expect(overlaySource()).toContain("oslStrip?.openQuickSettings()");
  });

  it("does not re-run a dismissed carrier after the storage is reopened", () => {
    const data = new Map<string, string>();
    const firstProfile = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => data.set(key, value) };
    const restartedProfile = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => data.set(key, value) };
    expect(stripCoachWasDismissed(firstProfile, "discord")).toBe(false);
    dismissStripCoach(firstProfile, "discord"); // SKIP or the final tip uses this same durable write.
    expect(stripCoachWasDismissed(restartedProfile, "discord")).toBe(true);
    expect(stripCoachWasDismissed(restartedProfile, "signal")).toBe(false);
  });

  it("tracks the live target and redraws the SVG arrow when layout changes", () => {
    const text = source();
    expect(text).toContain("target.getBoundingClientRect()");
    expect(text).toContain("new ResizeObserver(schedulePosition)");
    expect(text).toContain("new MutationObserver((records)");
    expect(text).toContain('data-strip-coach-arrow');
    expect(text).toContain("coachArrowTipX");
  });
});
