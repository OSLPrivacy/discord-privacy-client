// TASK 4760b - the three ways round the "Anyone" consent gate, each walked up
// to and found shut.
//
// Attempt 2 uses a real file in a real temporary directory: written, hand-
// edited to say `anyone`, read back off disk, restarted from, corrected on
// disk, and the reason appended to a real log file. Nothing here is a string
// standing in for a file.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  ANYONE_NEEDS_GATE,
  DISCOVERY_SETTING_CHOICES,
  clickOldStripCycler,
  createDiscoveryCardLedger,
  createDiscoverySettingStore,
  directAnyoneProbes,
  oldStripCyclerMarkup,
  oldStripCyclerNextValue,
  openAnyoneConsentGate,
  publishAfterRealConsent,
  publishDiscoveryCard,
  publishedCardCount,
  readDiscoverySetting,
  restartDiscoveryFromSettingsFileText,
  runDirectAnyoneProbe,
  serialiseDiscoverySettingsFile,
  setAnyoneConsentGateTick,
  turnOnAnyoneFromGate,
} from "./discovery-anyone-walkaround";
import type { DiscoverySetting } from "./discovery-anyone-walkaround";

const AT = "2026-08-07T18:20:00.000Z";
const CORRECTED_REASON = `${ANYONE_NEEDS_GATE} (stored setting corrected to never)`;
const OTHER_THREE: DiscoverySetting[] = ["never", "allowed", "shared-room"];

let profileDir = "";
beforeAll(() => {
  profileDir = mkdtempSync(join(tmpdir(), "osl-4760b-vitest-"));
});
afterAll(() => {
  rmSync(profileDir, { recursive: true, force: true });
});

describe("TASK 4760b attempt 1 - the setting-change path, called directly with anyone", () => {
  it("refuses every direct call and leaves the stored setting where it was, with 0 cards", () => {
    const rows: string[] = [];
    for (const startOn of OTHER_THREE) {
      for (const probe of directAnyoneProbes()) {
        const outcome = runDirectAnyoneProbe(probe, startOn, AT);
        expect(outcome.ok, `${outcome.probe} from ${startOn} was allowed`).toBe(false);
        expect(outcome.refusal).toBe(outcome.expect);
        expect(outcome.settingAfter).toBe(outcome.settingBefore);
        expect(outcome.settingAfter).not.toBe("anyone");
        expect(outcome.cardsPublished).toBe(0);
        rows.push(`${startOn}/${outcome.probe} refusal="${outcome.refusal}" before=${outcome.settingBefore} after=${outcome.settingAfter} equal=${outcome.settingAfter === outcome.settingBefore} cards=${outcome.cardsPublished}`);
      }
    }
    // The headline call: the setting-change path asked for `anyone` outright.
    const headline = runDirectAnyoneProbe(directAnyoneProbes()[0]!, "allowed", AT);
    expect(headline.refusal).toBe(ANYONE_NEEDS_GATE);
    console.log(
      `TASK4760B_ATTEMPT_1 calls=${rows.length} refusal="${headline.refusal}"`
      + ` exact_words=${headline.refusal === ANYONE_NEEDS_GATE}`
      + ` setting_before=${headline.settingBefore} setting_after=${headline.settingAfter}`
      + ` equal=${headline.settingAfter === headline.settingBefore} cards_published=${headline.cardsPublished}`,
    );
    console.log(`TASK4760B_ATTEMPT_1_CALLS ${rows.join(" | ")}`);
  });

  it("still writes the other three values, so the refusal is aimed at anyone alone", () => {
    const rows: string[] = [];
    for (const value of OTHER_THREE) {
      const store = createDiscoverySettingStore({ setting: "never" });
      const ledger = createDiscoveryCardLedger();
      const chosen = clickOldStripCyclerTo(store, value);
      publishDiscoveryCard(store, ledger, "@someone", AT);
      expect(chosen).toBe(value);
      expect(publishedCardCount(ledger)).toBe(0);
      rows.push(`${value}=written cards=0`);
    }
    console.log(`TASK4760B_OTHER_THREE_STILL_WRITE ${rows.join(" ")}`);
  });
});

/** Clicks the replica until it lands on `wanted`, and reports where it ended up. */
function clickOldStripCyclerTo(store: ReturnType<typeof createDiscoverySettingStore>, wanted: DiscoverySetting): DiscoverySetting {
  for (let click = 0; click < DISCOVERY_SETTING_CHOICES.length * 2; click += 1) {
    if (readDiscoverySetting(store) === wanted) break;
    clickOldStripCycler(store);
  }
  return readDiscoverySetting(store);
}

describe("TASK 4760b attempt 2 - anyone written straight into the stored settings file", () => {
  it("corrects the tampered file to never on restart, logs the reason, and publishes 0 cards", () => {
    const settingsPath = join(profileDir, "discovery-settings.json");
    const logPath = join(profileDir, "discovery-corrections.log");
    writeFileSync(logPath, "", "utf8");

    // The profile as it stood: a fresh one, on the real default.
    const honest = createDiscoverySettingStore({ setting: "never" });
    writeFileSync(settingsPath, serialiseDiscoverySettingsFile(honest), "utf8");
    const before = JSON.parse(readFileSync(settingsPath, "utf8")).setting;
    expect(before).toBe("never");

    // The tamper, by hand, straight into the file.
    writeFileSync(settingsPath, readFileSync(settingsPath, "utf8").replace('"never"', '"anyone"'), "utf8");
    const tampered = JSON.parse(readFileSync(settingsPath, "utf8")).setting;
    expect(tampered).toBe("anyone");

    // The restart.
    const restart = restartDiscoveryFromSettingsFileText(readFileSync(settingsPath, "utf8"), AT);
    if (restart.fileChanged) writeFileSync(settingsPath, restart.correctedText, "utf8");
    for (const line of restart.logLines) appendFileSync(logPath, `${line}\n`, "utf8");

    const ledger = createDiscoveryCardLedger();
    publishDiscoveryCard(restart.store, ledger, "@someone", AT);

    const onDisk = JSON.parse(readFileSync(settingsPath, "utf8")).setting;
    const logged = readFileSync(logPath, "utf8").trim();

    expect(restart.settingAfterRestart).toBe("never");
    expect(restart.settingAfterRestart).toBe(before);
    expect(onDisk).toBe("never");
    expect(restart.corrections).toEqual([CORRECTED_REASON]);
    expect(restart.corrections[0]!.startsWith(ANYONE_NEEDS_GATE)).toBe(true);
    expect(logged).toContain(ANYONE_NEEDS_GATE);
    expect(publishedCardCount(ledger)).toBe(0);

    // A second restart from the corrected file has nothing left to correct.
    const again = restartDiscoveryFromSettingsFileText(readFileSync(settingsPath, "utf8"), AT);
    expect(again.settingAfterRestart).toBe("never");
    expect(again.corrections).toEqual([]);

    console.log(
      `TASK4760B_ATTEMPT_2 refusal="${restart.corrections[0]}"`
      + ` exact_words=${restart.corrections[0]!.startsWith(ANYONE_NEEDS_GATE)}`
      + ` setting_before=${before} setting_after=${restart.settingAfterRestart}`
      + ` equal=${restart.settingAfterRestart === before} cards_published=${publishedCardCount(ledger)}`,
    );
    console.log(
      `TASK4760B_ATTEMPT_2_FILE on_disk_before="${before}" on_disk_tampered="${tampered}"`
      + ` on_disk_after_restart="${onDisk}" corrected_to_never=${onDisk === "never"}`
      + ` file_rewritten=${restart.fileChanged} second_restart=${again.settingAfterRestart}`
      + ` second_restart_corrections=${again.corrections.length}`,
    );
    console.log(`TASK4760B_ATTEMPT_2_LOG "${logged}"`);
  });

  it("corrects every shape of a hand-written anyone, always to never", () => {
    const rows: string[] = [];
    for (const [shape, text] of [
      ["anyone, no stamp", JSON.stringify({ setting: "anyone" })],
      ["anyone, null stamp", JSON.stringify({ setting: "anyone", consentStamp: null })],
      ["anyone, unsealed stamp", JSON.stringify({ setting: "anyone", consentStamp: { recordedDate: AT, wordingVersion: "anyone-consent-v1+cdd02ec0" } })],
      ["anyone, forged seal", JSON.stringify({ setting: "anyone", consentStamp: { recordedDate: AT, wordingVersion: "anyone-consent-v1+cdd02ec0", seal: "deadbeef" } })],
      ["anyone, stamp with no date", JSON.stringify({ setting: "anyone", consentStamp: { wordingVersion: "anyone-consent-v1+cdd02ec0", seal: "e2dee5a1" } })],
      ["anyone, stamp is a string", JSON.stringify({ setting: "anyone", consentStamp: "yes" })],
      ["ANYONE in capitals", JSON.stringify({ setting: "ANYONE" })],
      ["anyone with a trailing space", JSON.stringify({ setting: "anyone " })],
      ["not JSON at all", "setting=anyone"],
      ["an array", "[\"anyone\"]"],
    ] as const) {
      const restart = restartDiscoveryFromSettingsFileText(text, AT);
      const ledger = createDiscoveryCardLedger();
      publishDiscoveryCard(restart.store, ledger, "@someone", AT);
      expect(restart.settingAfterRestart).toBe("never");
      expect(publishedCardCount(ledger)).toBe(0);
      rows.push(`${shape}->${restart.settingAfterRestart} cards=0`);
    }
    console.log(`TASK4760B_ATTEMPT_2_SHAPES ${rows.length} shapes | ${rows.join(" | ")}`);
  });

  it("a file holding a real gate-issued stamp survives the restart, so the correction is not blanket", () => {
    // Round-trip a store that really went through the gate: file, and back.
    const consented = createDiscoverySettingStore({ setting: "allowed" });
    const gateStore = consentedStore(consented);
    const text = serialiseDiscoverySettingsFile(gateStore);
    const restart = restartDiscoveryFromSettingsFileText(text, AT);
    const ledger = createDiscoveryCardLedger();
    publishDiscoveryCard(restart.store, ledger, "@someone", AT);
    expect(restart.settingAfterRestart).toBe("anyone");
    expect(restart.corrections).toEqual([]);
    expect(publishedCardCount(ledger)).toBe(1);
    console.log(
      `TASK4760B_REAL_STAMP_SURVIVES_RESTART setting_after=${restart.settingAfterRestart}`
      + ` corrections=${restart.corrections.length} cards_published=${publishedCardCount(ledger)}`,
    );
  });
});

/** A store that really went through the gate: opened, ticked, turned on. */
function consentedStore(store: ReturnType<typeof createDiscoverySettingStore>) {
  const gate = setAnyoneConsentGateTick(openAnyoneConsentGate(store), true, AT);
  const result = turnOnAnyoneFromGate(gate, store);
  expect(result.ok).toBe(true);
  return store;
}

describe("TASK 4760b attempt 3 - the old Strip cycler", () => {
  it("cycles through the other three but is refused at anyone, with 0 cards", () => {
    const store = createDiscoverySettingStore({ setting: "never" });
    const ledger = createDiscoveryCardLedger();
    const clicks: string[] = [];
    let atAnyone: { before: DiscoverySetting; refusal: string; after: DiscoverySetting } | null = null;
    for (let click = 0; click < 6; click += 1) {
      const before = readDiscoverySetting(store);
      const result = clickOldStripCycler(store);
      publishDiscoveryCard(store, ledger, "@someone", AT);
      const after = readDiscoverySetting(store);
      if (result.asked === "anyone") {
        expect(result.ok).toBe(false);
        expect(result.refusal).toBe(ANYONE_NEEDS_GATE);
        expect(after).toBe(before);
        atAnyone = { before, refusal: result.refusal, after };
      }
      clicks.push(`click${click + 1} from=${result.from} asked=${result.asked} ok=${result.ok} after=${after}`);
    }
    expect(atAnyone).not.toBeNull();
    expect(readDiscoverySetting(store)).toBe("shared-room");
    expect(publishedCardCount(ledger)).toBe(0);
    console.log(
      `TASK4760B_ATTEMPT_3 clicks=${clicks.length} refusal="${atAnyone!.refusal}"`
      + ` exact_words=${atAnyone!.refusal === ANYONE_NEEDS_GATE}`
      + ` setting_before=${atAnyone!.before} setting_after=${atAnyone!.after}`
      + ` equal=${atAnyone!.after === atAnyone!.before} cards_published=${publishedCardCount(ledger)}`,
    );
    console.log(`TASK4760B_ATTEMPT_3_CLICKS ${clicks.join(" | ")}`);
  });

  it("is stuck on shared-room however many times it is clicked", () => {
    const store = createDiscoverySettingStore({ setting: "shared-room" });
    const ledger = createDiscoveryCardLedger();
    for (let click = 0; click < 500; click += 1) {
      clickOldStripCycler(store);
      publishDiscoveryCard(store, ledger, "@someone", AT);
    }
    expect(readDiscoverySetting(store)).toBe("shared-room");
    expect(publishedCardCount(ledger)).toBe(0);
    console.log(
      `TASK4760B_ATTEMPT_3_HAMMERED clicks=500 setting_after=${readDiscoverySetting(store)}`
      + ` cards_published=${publishedCardCount(ledger)}`,
    );
  });

  it("carries the cycle order the old control had, and paints its refusal on screen", () => {
    expect(DISCOVERY_SETTING_CHOICES.map((v) => oldStripCyclerNextValue(v)))
      .toEqual(["allowed", "shared-room", "anyone", "never"]);
    const store = createDiscoverySettingStore({ setting: "shared-room" });
    const click = clickOldStripCycler(store);
    const markup = oldStripCyclerMarkup(store, click.refusal);
    expect(markup).toContain(ANYONE_NEEDS_GATE);
    expect(markup).toContain('data-setting="shared-room"');
    expect(markup).toContain('data-next="anyone"');
    console.log(
      `TASK4760B_ATTEMPT_3_ON_SCREEN refusal_in_markup=${markup.includes(ANYONE_NEEDS_GATE)}`
      + ` data_setting=shared-room data_next=anyone`,
    );
  });
});

describe("TASK 4760b - the card counter is not decoration", () => {
  it("publishes exactly 1 card once the gate really is ticked and turned on", () => {
    const live = publishAfterRealConsent("allowed", AT);
    expect(live.settingBefore).toBe("allowed");
    expect(live.settingAfter).toBe("anyone");
    expect(live.cardsPublished).toBe(1);
    expect(live.wordingVersion).toBe("anyone-consent-v1+cdd02ec0");
    console.log(
      `TASK4760B_CARD_COUNTER_LIVE setting_before=${live.settingBefore} setting_after=${live.settingAfter}`
      + ` cards_published=${live.cardsPublished} wording_version=${live.wordingVersion}`,
    );
  });

  it("publishes nothing on any of the other three values", () => {
    const rows: string[] = [];
    for (const value of OTHER_THREE) {
      const store = createDiscoverySettingStore({ setting: value });
      const ledger = createDiscoveryCardLedger();
      const result = publishDiscoveryCard(store, ledger, "@someone", AT);
      expect(result.ok).toBe(false);
      expect(publishedCardCount(ledger)).toBe(0);
      rows.push(`${value}=0`);
    }
    console.log(`TASK4760B_CARDS_ON_OTHER_VALUES ${rows.join(" ")}`);
  });
});
