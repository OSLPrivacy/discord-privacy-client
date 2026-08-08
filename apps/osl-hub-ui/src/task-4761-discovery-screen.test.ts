import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  DISCOVERY_CHOICES,
  DISCOVERY_DISCLOSURE_SENTENCE,
  DISCOVERY_PINGS_LABEL,
  DISCOVERY_STRIP_ROW_NAME,
  DISCOVERY_VISIBILITY_STORAGE_KEY,
  defaultDiscoveryVisibilityState,
  discoveryChoiceLabels,
  discoveryChoiceOrder,
  discoveryChoiceRows,
  discoveryStripRow,
  readSavedDiscoveryVisibility,
  selectDiscoveryChoice,
  type DiscoveryVisibilityState,
} from "./discovery-visibility";
import { discoveryStripRowMarkup, discoveryVisibilityBody } from "./discovery-visibility-screen";

/**
 * TASK 4761. The BEING SEEN AS AN OSL USER screen, in ruling A10's order, with
 * the dead reference copy gone.
 *
 * The two forbidden strings are written out here and only here. This file is a
 * test, not a screen, so the scan below skips `*.test.ts` on purpose: a guard
 * that had to be blind to its own wording could not name what it forbids.
 */
const DEAD_STRIP_CYCLER = "Findable by strangers";
const DEAD_ANYONE_RADIO_LABEL = "Show me to anyone";

const here = dirname(fileURLToPath(import.meta.url));
const appRoot = join(here, "..");

/**
 * Everything that renders a shipped screen: the renderer sources, their styles,
 * and the HTML documents that host them. Tests are excluded, as are `dist`,
 * `node_modules` and `screenshots`, which are build output and tooling.
 */
function shippedScreenFiles(): string[] {
  const skipDirectories = new Set(["node_modules", "dist", "screenshots", "assets"]);
  const found: string[] = [];
  const walk = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const full = join(directory, entry.name);
      if (entry.isDirectory()) {
        if (!skipDirectories.has(entry.name)) walk(full);
        continue;
      }
      if (entry.name.endsWith(".test.ts") || entry.name.endsWith(".test.mjs")) continue;
      if (/\.(ts|css|html)$/u.test(entry.name)) found.push(full);
    }
  };
  walk(join(appRoot, "src"));
  for (const entry of readdirSync(appRoot, { withFileTypes: true })) {
    if (entry.isFile() && entry.name.endsWith(".html")) found.push(join(appRoot, entry.name));
  }
  return found.sort();
}

function countHits(needle: string): { hits: number; files: string[] } {
  const files = shippedScreenFiles().filter((file) => readFileSync(file, "utf8").includes(needle));
  return { hits: files.length, files };
}

describe("TASK 4761 — being seen as an OSL user", () => {
  it("shows exactly 4 choices in A10's order with A10's labels", () => {
    const labels = discoveryChoiceLabels();
    labels.forEach((label, index) => console.log(`choice ${index + 1}: ${label}`));
    console.log(`saved words in order: ${discoveryChoiceOrder().join(", ")}`);
    console.log(`choice count: ${labels.length}`);

    expect(labels).toEqual([
      "Never show me",
      "Only people I've allowed",
      "Anyone I've shared a chat with",
      "Anyone",
    ]);
    expect(labels).toHaveLength(4);
    expect(discoveryChoiceOrder()).toEqual(["never", "allowed", "shared-room", "anyone"]);
  });

  it("renders those 4 labels on screen, in that order, and no fifth", () => {
    const markup = discoveryVisibilityBody(defaultDiscoveryVisibilityState());
    const positions = discoveryChoiceLabels().map((label) => markup.indexOf(`<strong>${label.replaceAll("'", "&#39;")}</strong>`));
    positions.forEach((position, index) => console.log(`rendered choice ${index + 1} at offset ${position}`));

    expect(positions.every((position) => position >= 0)).toBe(true);
    expect([...positions]).toEqual([...positions].sort((left, right) => left - right));
    const renderedRadios = markup.match(/data-discovery-choice="/gu) ?? [];
    console.log(`rendered radios: ${renderedRadios.length}`);
    expect(renderedRadios).toHaveLength(4);
  });

  it("selects choice 1 on a fresh profile", () => {
    const fresh = defaultDiscoveryVisibilityState();
    const rows = discoveryChoiceRows(fresh);
    const selected = rows.filter((row) => row.checked);
    console.log(`fresh profile: choice ${selected[0]?.position} (${selected[0]?.choice.label}) selected, ${selected.length} selected in total`);
    console.log(`fresh profile: reply to discovery pings = ${fresh.replyToPings ? "on" : "off"}`);

    expect(fresh.choice).toBe("never");
    expect(fresh.replyToPings).toBe(false);
    expect(selected).toHaveLength(1);
    expect(selected[0]?.position).toBe(1);

    const markup = discoveryVisibilityBody(fresh);
    const checked = markup.match(/data-discovery-choice="[a-z-]+" checked/gu) ?? [];
    expect(checked).toEqual(['data-discovery-choice="never" checked']);
    // An empty profile folder reads the same way as a fresh one.
    expect(readSavedDiscoveryVisibility(null)).toEqual(fresh);
  });

  it("finds 0 hits for the dead Strip cycler and 0 for the dead anyone radio", () => {
    const scanned = shippedScreenFiles();
    const cycler = countHits(DEAD_STRIP_CYCLER);
    const radio = countHits(DEAD_ANYONE_RADIO_LABEL);
    console.log(`shipped screen files scanned: ${scanned.length}`);
    console.log(`hits for "${DEAD_STRIP_CYCLER}": ${cycler.hits} ${cycler.files.join(", ")}`);
    console.log(`hits for "${DEAD_ANYONE_RADIO_LABEL}": ${radio.hits} ${radio.files.join(", ")}`);

    expect(scanned.length).toBeGreaterThan(0);
    expect(cycler.hits).toBe(0);
    expect(radio.hits).toBe(0);
  });

  it("keeps the reply-to-discovery-pings switch under the choices", () => {
    const markup = discoveryVisibilityBody(defaultDiscoveryVisibilityState());
    const lastChoice = markup.indexOf('data-discovery-choice="anyone"');
    const pings = markup.indexOf(DISCOVERY_PINGS_LABEL);
    console.log(`last choice at ${lastChoice}, pings switch at ${pings}`);

    expect(lastChoice).toBeGreaterThan(-1);
    expect(pings).toBeGreaterThan(lastChoice);
    expect(markup).toContain("Off means OSL never answers &#39;are you on OSL?&#39;");
  });

  it("prints the disclosure sentence from 4762 under the choices, character for character", () => {
    // TASK 4762's finish line: the one-line honest summary at the bottom of the
    // watcher table becomes the disclosure sentence on the settings screen.
    const fromTask4762 = "If you choose Anyone, a stranger holding your handle learns yes, and that cannot be un-learned.";
    const markup = discoveryVisibilityBody(defaultDiscoveryVisibilityState());
    const onScreen = /<p class="discovery-disclosure" data-discovery-disclosure>([^<]*)<\/p>/u.exec(markup)?.[1] ?? "";
    console.log(`disclosure on screen: ${onScreen}`);
    console.log(`matches 4762 exactly: ${onScreen === fromTask4762}`);

    expect(DISCOVERY_DISCLOSURE_SENTENCE).toBe(fromTask4762);
    expect(onScreen).toBe(fromTask4762);
    const disclosureAt = markup.indexOf("discovery-disclosure");
    expect(disclosureAt).toBeGreaterThan(markup.indexOf('data-discovery-choice="anyone"'));
  });

  it("counts what clicking the Strip row does: 0 setting changes, 1 Settings opened", () => {
    let stored: DiscoveryVisibilityState = { choice: "allowed", replyToPings: true };
    let writes = 0;
    let opened = 0;
    const store = {
      read: () => stored,
      write: (next: DiscoveryVisibilityState) => {
        writes += 1;
        stored = next;
      },
    };
    const row = discoveryStripRow(store, () => {
      opened += 1;
    });
    const before = stored.choice;

    row.activate();

    console.log(`strip row: name=${row.name} value=${row.value} readOnly=${row.readOnly}`);
    console.log(`stored setting changed ${writes} times`);
    console.log(`Settings opened ${opened} times`);
    console.log(`stored setting before=${before} after=${stored.choice}`);

    expect(writes).toBe(0);
    expect(opened).toBe(1);
    expect(stored.choice).toBe(before);
    expect(row.readOnly).toBe(true);
    expect(row.value).toBe("Only people I've allowed");
    expect(row.opensSettingsSection).toBe("discovery");
    expect(row.name).toBe(DISCOVERY_STRIP_ROW_NAME);
  });

  it("renders the Strip row as a read-out with no control that can change the setting", () => {
    const markup = discoveryStripRowMarkup({ choice: "shared-room", replyToPings: false });
    console.log(`strip row markup: ${markup}`);

    expect(markup).toContain("Anyone I&#39;ve shared a chat with");
    expect(markup).toContain('data-settings="discovery"');
    expect(markup).toContain('data-route="settings"');
    expect(markup).not.toContain("data-discovery-choice=");
    expect(markup).not.toContain("<input");
  });

  it("refuses a fifth value, and refuses Anyone without the consent gate", () => {
    const fresh = defaultDiscoveryVisibilityState();
    expect(() => selectDiscoveryChoice(fresh, "sometimes")).toThrow("unknown discovery setting sometimes");
    expect(() => selectDiscoveryChoice(fresh, "anyone")).toThrow("discovery: anyone needs the consent gate");
    expect(selectDiscoveryChoice(fresh, "anyone", { consentGatePassed: true }).choice).toBe("anyone");
    // A tampered profile file must not restore a gated value.
    const tampered = readSavedDiscoveryVisibility(JSON.stringify({ choice: "anyone", replyToPings: true }));
    console.log(`tampered file read back as: ${tampered.choice}`);
    expect(tampered.choice).toBe("never");
    expect(DISCOVERY_VISIBILITY_STORAGE_KEY).toBe("osl.discovery-visibility");
    expect(DISCOVERY_CHOICES.filter((choice) => choice.needsConsentGate).map((choice) => choice.id)).toEqual(["anyone"]);
  });
});
