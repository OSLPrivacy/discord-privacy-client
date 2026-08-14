import { describe, expect, it } from "vitest";
import {
  AUTOSCRUB_PRO_LOCK_LINE,
  discoveryDoneLine,
  planDiscoveryRun,
  resetScrubDiscoveryScreen,
  scrubDiscoveryScreenMarkup,
  shippingDiscoveryReader,
} from "./scrub-discovery-screen";
import type { ScrubAccountRow } from "./scrub-account-choice";

const discord: ScrubAccountRow = {
  serviceId: "discord",
  accountId: "discord-maple",
  accountLabel: "kestrel#0042",
  appOrBrowserLabel: "Discord app",
};

const gmail: ScrubAccountRow = {
  serviceId: "email",
  accountId: "gmail-birch",
  accountLabel: "kestrel@gmail.com",
  appOrBrowserLabel: "Firefox",
};

describe("the discovery console plan", () => {
  it("refuses an empty selection without reading anything", () => {
    const plan = planDiscoveryRun([], shippingDiscoveryReader);
    expect(plan.lines.map((line) => line.text)).toEqual([
      "scrub start · discovery · local only",
      "no accounts allowed · allow at least one account first",
      "stopped · nothing was read",
    ]);
    expect(plan.totalExposures).toBe(0);
    expect(plan.readCount).toBe(0);
  });

  it("never invents counts with the shipping (null) reader", () => {
    const plan = planDiscoveryRun([discord, gmail], shippingDiscoveryReader);
    const texts = plan.lines.map((line) => line.text);
    expect(texts).toContain("discord · kestrel#0042 · account reader is not in this build · nothing was read");
    expect(texts).toContain("email · kestrel@gmail.com · account reader is not in this build · nothing was read");
    // No fabricated per-account counts appear anywhere.
    expect(texts.join("\n")).not.toMatch(/\d+ items readable/u);
    expect(texts.join("\n")).not.toMatch(/\d+ exposures look public/u);
    expect(plan.readCount).toBe(0);
    // The closing line states what actually happened, including the zero.
    expect(texts.at(-1)).toBe("done · 0 public exposures found · nothing deleted · deletion is AutoScrub (Pro)");
  });

  it("streams the spec lines when a reader supplies real counts", () => {
    const plan = planDiscoveryRun([discord, gmail], (account) =>
      account.serviceId === "discord" ? { readable: 412, exposures: 3 } : { readable: 1204, exposures: 11 });
    const texts = plan.lines.map((line) => line.text);
    expect(texts).toContain("discord · kestrel#0042 · 412 items readable · credentials untouched");
    expect(texts).toContain("discord · kestrel#0042 · 3 exposures look public");
    expect(texts).toContain("email · kestrel@gmail.com · 1204 items readable · credentials untouched");
    expect(texts).toContain("email · kestrel@gmail.com · 11 exposures look public");
    expect(texts.at(-1)).toBe(discoveryDoneLine(14));
    expect(texts.at(-1)).toBe("done · 14 public exposures found · nothing deleted · deletion is AutoScrub (Pro)");
    expect(plan.readCount).toBe(2);
  });

  it("keeps the exact free-tier AutoScrub refusal line", () => {
    expect(AUTOSCRUB_PRO_LOCK_LINE).toBe("autoscrub is a Pro feature · discovery stays free");
  });
});

describe("the discovery screen markup", () => {
  it("carries the canonical copy and no consent gate on the discovery path", () => {
    resetScrubDiscoveryScreen();
    const markup = scrubDiscoveryScreenMarkup(false);
    expect(markup).toContain("Find what of yours is already out there");
    expect(markup).toContain("Reads your own machine, builds the list. Deleting is separate, reviewed, and never the default.");
    expect(markup).toContain("CONSOLE");
    expect(markup).toContain("Run discovery");
    expect(markup).toContain("Never logs in for you · never reads saved passwords");
    // Discovery deletes nothing, so it renders no consent gate of any kind.
    expect(markup).not.toContain("scrub-consent-gate");
    expect(markup).not.toContain("scrub-consent-page");
    // Nothing typed gates anything on this screen (design rule 4).
    expect(markup).not.toContain("typedAcknowledgement");
    expect(markup).not.toContain('type="text"');
  });

  it("keeps a hover reason on the PRO-locked AutoScrub row", () => {
    resetScrubDiscoveryScreen();
    const markup = scrubDiscoveryScreenMarkup(false);
    expect(markup).toContain('data-sd-autoscrub aria-disabled="true" title="AutoScrub is a Pro feature · discovery stays free"');
    expect(markup).toContain('<span class="sd-pro-tag">PRO</span>');
  });
});
