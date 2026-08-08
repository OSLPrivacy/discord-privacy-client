import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { composerLockAvailability, ComposerProtectionTraceController } from "./composer-protection-trace";

const overlay = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");
const markup = readFileSync(new URL("../overlay.html", import.meta.url), "utf8");
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("TASK 5022 composer protection trace", () => {
  it("traces exactly once for each accepted off-to-on edge", () => {
    const controller = new ComposerProtectionTraceController();
    const exactCounts: number[] = [controller.snapshot().traceCount];
    const offResidue: boolean[] = [];

    // Duplicate on announcements are expected from the retained overlay and
    // must not restart its CSS animation.
    for (let cycle = 0; cycle < 5; cycle += 1) {
      controller.applyLockEngaged(true, true);
      exactCounts.push(controller.snapshot().traceCount);
      controller.applyLockEngaged(true, true);
      exactCounts.push(controller.snapshot().traceCount);
      controller.applyLockEngaged(false, true);
      offResidue.push(controller.snapshot().engaged);
    }

    console.info(`TASK 5022 exact trace counts: ${exactCounts.join(",")}`);
    console.info(`TASK 5022 off residue: ${offResidue.filter(Boolean).length}`);
    expect(exactCounts).toEqual([0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5]);
    expect(offResidue).toEqual([false, false, false, false, false]);
    expect(controller.snapshot()).toEqual({ engaged: false, traceCount: 5 });
  });

  it("does not trace an unavailable Discord composer", () => {
    const controller = new ComposerProtectionTraceController();
    controller.applyLockEngaged(true, false);
    const lock = composerLockAvailability(false, false);
    console.info(`TASK 5022 unavailable lock: disabled=${lock.disabled} tone=${lock.tone} reason="${lock.reason}" trace count=${controller.snapshot().traceCount}`);
    expect(controller.snapshot()).toEqual({ engaged: false, traceCount: 0 });
    expect(lock).toEqual({
      unavailable: true,
      disabled: true,
      className: " composer-unavailable",
      tone: "grey",
      reason: "No Discord composer is detected.",
    });
    expect(main).toContain("composerLockAvailability(discordMarkerAvailable, nativeDiscordProtectionActive)");
    expect(main).toContain("composerAvailability.className");
    expect(main).toContain("discordQaComposerBusy || composerAvailability.disabled");
    expect(main).toContain('data-lock-state="${composerLockState}"');
  });

  it("binds one non-looping trace and the steady outline to the measured composer box", () => {
    const composerStart = markup.indexOf('<div class="composer-box">');
    const traceStart = markup.indexOf('<svg class="composer-protection-trace"');
    const composerEnd = markup.indexOf("</div>", traceStart);
    expect(composerStart).toBeGreaterThan(-1);
    expect(traceStart).toBeGreaterThan(composerStart);
    expect(composerEnd).toBeGreaterThan(traceStart);
    expect(markup).toContain('<rect pathLength="1"></rect>');

    expect(styles).toMatch(/\.composer-protection-trace\s*\{[^}]*position:\s*absolute;[^}]*inset:\s*0;/su);
    expect(styles).toMatch(/\.composer-protection-trace rect\s*\{[^}]*stroke-dasharray:\s*1;[^}]*stroke-dashoffset:\s*1;[^}]*opacity:\s*0;/su);
    expect(styles).toMatch(/\.composer-protection-trace rect\s*\{[^}]*width:\s*calc\(100% - 1px\);[^}]*height:\s*calc\(100% - 1px\);[^}]*rx:\s*8px;/su);
    expect(styles).toMatch(/\.composer-protection-trace\.composer-protection-tracing rect\s*\{[^}]*animation-iteration-count:\s*1;/su);
    expect(styles).not.toMatch(/animation-iteration-count:\s*infinite/u);
    expect(styles).toMatch(/\.composer-box::after\s*\{[^}]*border:\s*1px solid rgba\(73, 214, 255, \.55\);[^}]*opacity:\s*0;/su);
    expect(styles).toMatch(/\.composer-box\.composer-protection-active::after\s*\{\s*opacity:\s*1;/su);

    expect(overlay).toContain('composerBox.classList.toggle("composer-protection-active", engaged)');
    expect(overlay).toContain('composerProtectionTrace.classList.remove("composer-protection-tracing")');
    expect(overlay).toContain('composerProtectionTrace.classList.add("composer-protection-tracing")');
    expect(overlay).toContain("applyLockEngaged(!payload);");
    expect(overlay).toMatch(/discordMarkerAvailable = state\.discordMarkerAvailable;\s*applyLockEngaged\(state\.lockEngaged \?\? true\);/u);
    console.info("TASK 5022 animation iterations: 1; steady outline: 1; composer target: .composer-box");
  });
});
