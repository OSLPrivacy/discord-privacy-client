// TASK 4760 - the consent gate in front of "Anyone".
//
// Every clause of the finish line is checked here and printed with the number
// or string it produced:
//
//   1. all three sentences present character for character -> "3 of 3 found"
//   2. Turn on greyed while unticked, live after
//   3. leaving the gate any other way leaves the stored setting where it was
//   4. the consent stamp holds a date and a wording version, and is refused
//      without either one
//   5. exactly one code path can move the stored setting to `anyone` -- checked
//      here by calling every export of the module against a fresh store, and
//      statically by scripts/task-4760-anyone-path-search.mjs
import { describe, expect, it } from "vitest";
import * as gateModule from "./discovery-anyone-consent-gate";
import {
  ANYONE_CONSENT_SENTENCES,
  ANYONE_CONSENT_WORDING_VERSION,
  ANYONE_NEEDS_GATE,
  CONSENT_STAMP_NEEDS_BOTH,
  DISCOVERY_SETTING_CHOICES,
  TURN_ON_NEEDS_TICK,
  anyoneConsentGateMarkup,
  anyoneConsentStampIsWhole,
  anyoneConsentTurnOnAvailable,
  chooseDiscoverySetting,
  createDiscoverySettingStore,
  leaveAnyoneConsentGate,
  openAnyoneConsentGate,
  readDiscoverySetting,
  recordAnyoneConsentStamp,
  setAnyoneConsentGateTick,
  turnOnAnyoneFromGate,
  wordingVersionFor,
  type AnyoneConsentGate,
  type DiscoverySettingStore,
} from "./discovery-anyone-consent-gate";

const TICKED_AT = "2026-08-07T18:20:00.000Z";

/** The three sentences as the task wrote them, retyped here on purpose. */
const SENTENCES_FROM_THE_TASK = [
  "Anyone who knows your handle can learn you run OSL.",
  "That includes the carrier itself, which can check its whole history.",
  "This is retroactive and permanent. Turning it off later removes your record, but it does not un-tell anyone who already looked.",
];

function gateFrom(setting: DiscoverySetting): { store: DiscoverySettingStore; gate: AnyoneConsentGate } {
  const store = createDiscoverySettingStore({ setting });
  const chosen = chooseDiscoverySetting(store, "anyone");
  expect(chosen.ok).toBe(false);
  if (chosen.ok) throw new Error("unreachable");
  expect(chosen.gate).toBeDefined();
  return { store, gate: chosen.gate as AnyoneConsentGate };
}

type DiscoverySetting = (typeof DISCOVERY_SETTING_CHOICES)[number];

const turnOnIsGreyed = (gate: AnyoneConsentGate) =>
  anyoneConsentGateMarkup(gate).includes('id="discovery-anyone-gate-turn-on" type="button" disabled');

describe("TASK 4760 the consent gate in front of Anyone", () => {
  it("shows all three sentences character for character", () => {
    const { gate } = gateFrom("allowed");
    const markup = anyoneConsentGateMarkup(gate);
    const found = SENTENCES_FROM_THE_TASK.filter((sentence) => markup.includes(sentence));
    // Exact string match, not a fuzzy or normalised one.
    for (const sentence of SENTENCES_FROM_THE_TASK) expect(markup).toContain(sentence);
    expect(found.length).toBe(3);
    expect([...ANYONE_CONSENT_SENTENCES]).toEqual(SENTENCES_FROM_THE_TASK);
    console.log(`TASK4760_SENTENCES ${found.length} of ${SENTENCES_FROM_THE_TASK.length} found`);
    console.log(
      `TASK4760_SENTENCE_LENGTHS ${SENTENCES_FROM_THE_TASK.map((s, i) => `${i}=${s.length}`).join(" ")}`,
    );
  });

  it("has one tick box that starts unticked and no typed word", () => {
    const { gate } = gateFrom("never");
    const markup = anyoneConsentGateMarkup(gate);
    const tickBoxes = markup.match(/type="checkbox"/gu) ?? [];
    const textBoxes = markup.match(/type="text"|<textarea|<input(?![^>]*type="checkbox")/gu) ?? [];
    expect(tickBoxes.length).toBe(1);
    expect(textBoxes.length).toBe(0);
    expect(gate.ticked).toBe(false);
    expect(markup).not.toContain(" checked");
    console.log(
      `TASK4760_CONTROLS tick_boxes=${tickBoxes.length} starts_ticked=${gate.ticked}`
      + ` typed_word_boxes=${textBoxes.length}`,
    );
  });

  it("greys Turn on while the box is unticked and makes it live after", () => {
    const { gate } = gateFrom("allowed");
    const before = { greyed: turnOnIsGreyed(gate), available: anyoneConsentTurnOnAvailable(gate) };
    const ticked = setAnyoneConsentGateTick(gate, true, TICKED_AT);
    const after = { greyed: turnOnIsGreyed(ticked), available: anyoneConsentTurnOnAvailable(ticked) };
    const unticked = setAnyoneConsentGateTick(ticked, false, TICKED_AT);
    expect(before.greyed).toBe(true);
    expect(before.available).toBe(false);
    expect(after.greyed).toBe(false);
    expect(after.available).toBe(true);
    expect(turnOnIsGreyed(unticked)).toBe(true);
    console.log(
      `TASK4760_TURN_ON unticked_greyed=${before.greyed} unticked_available=${before.available}`
      + ` ticked_greyed=${after.greyed} ticked_available=${after.available}`
      + ` unticked_again_greyed=${turnOnIsGreyed(unticked)}`,
    );
  });

  it("refuses Turn on while the box is unticked, even called directly", () => {
    const { store, gate } = gateFrom("allowed");
    const before = readDiscoverySetting(store);
    const refused = turnOnAnyoneFromGate(gate, store);
    const after = readDiscoverySetting(store);
    expect(refused).toEqual({ ok: false, refusal: TURN_ON_NEEDS_TICK });
    expect(after).toBe(before);
    console.log(
      `TASK4760_DIRECT_TURN_ON_NO_TICK ok=${refused.ok} refusal="${refused.ok ? "" : refused.refusal}"`
      + ` setting_before=${before} setting_after=${after} equal=${before === after}`,
    );
  });

  it("leaves the stored setting alone when the gate is left any other way", () => {
    const lines: string[] = [];
    for (const startingOn of ["never", "allowed", "shared-room"] as const) {
      for (const how of ["back", "escape", "close", "other-choice", "walked-off"]) {
        const { store, gate } = gateFrom(startingOn);
        const before = readDiscoverySetting(store);
        // The worst case: tick it first, then leave any other way.
        const ticked = setAnyoneConsentGateTick(gate, true, TICKED_AT);
        const left = leaveAnyoneConsentGate(ticked, how);
        const after = readDiscoverySetting(store);
        const retry = turnOnAnyoneFromGate(left, store);
        const afterRetry = readDiscoverySetting(store);
        expect(after).toBe(before);
        expect(afterRetry).toBe(before);
        expect(retry.ok).toBe(false);
        expect(store.consentStamp).toBe(null);
        lines.push(`${startingOn}/${how} before=${before} after=${after} equal=${before === after}`);
      }
    }
    console.log(`TASK4760_LEFT_ANY_OTHER_WAY ${lines.join(" | ")}`);
    expect(lines.length).toBe(15);
  });

  it("records a consent stamp with a date and a wording version, and refuses either half missing", () => {
    const { store, gate } = gateFrom("allowed");
    const before = readDiscoverySetting(store);
    const ticked = setAnyoneConsentGateTick(gate, true, TICKED_AT);
    const turnedOn = turnOnAnyoneFromGate(ticked, store);
    expect(turnedOn.ok).toBe(true);
    if (!turnedOn.ok) throw new Error("unreachable");
    expect(turnedOn.stamp.recordedDate).toBe(TICKED_AT);
    expect(turnedOn.stamp.wordingVersion).toBe(ANYONE_CONSENT_WORDING_VERSION);
    expect(wordingVersionFor(ANYONE_CONSENT_SENTENCES)).toBe(ANYONE_CONSENT_WORDING_VERSION);
    expect(store.consentStamp).toEqual(turnedOn.stamp);
    expect(readDiscoverySetting(store)).toBe("anyone");
    console.log(
      `TASK4760_STAMP setting_before=${before} setting_after=${readDiscoverySetting(store)}`
      + ` date=${turnedOn.stamp.recordedDate} wording_version=${turnedOn.stamp.wordingVersion}`,
    );

    const halves: string[] = [];
    for (const [name, input] of [
      ["no_date", { wordingVersion: ANYONE_CONSENT_WORDING_VERSION }],
      ["empty_date", { recordedDate: "", wordingVersion: ANYONE_CONSENT_WORDING_VERSION }],
      ["unreadable_date", { recordedDate: "some time last week", wordingVersion: ANYONE_CONSENT_WORDING_VERSION }],
      ["no_wording_version", { recordedDate: TICKED_AT }],
      ["empty_wording_version", { recordedDate: TICKED_AT, wordingVersion: "" }],
      ["unreadable_wording_version", { recordedDate: TICKED_AT, wordingVersion: "latest" }],
      ["neither", {}],
    ] as const) {
      let refusal = "WROTE ANYWAY";
      try {
        recordAnyoneConsentStamp(input);
      } catch (error) {
        refusal = error instanceof Error ? error.message : String(error);
      }
      expect(refusal).toBe(CONSENT_STAMP_NEEDS_BOTH);
      halves.push(`${name}="${refusal}"`);
    }
    console.log(`TASK4760_STAMP_REFUSALS ${halves.join(" ")}`);

    // A draft written by hand is not a stamp: only ticking the box seals one.
    const draft = recordAnyoneConsentStamp({
      recordedDate: TICKED_AT,
      wordingVersion: ANYONE_CONSENT_WORDING_VERSION,
    });
    expect(anyoneConsentStampIsWhole(draft)).toBe(false);
    expect(anyoneConsentStampIsWhole(turnedOn.stamp)).toBe(true);
    expect(anyoneConsentStampIsWhole({ ...turnedOn.stamp, seal: "00000000" })).toBe(false);
    expect(anyoneConsentStampIsWhole({ ...turnedOn.stamp, recordedDate: "2026-08-08T00:00:00.000Z" })).toBe(false);
    console.log(
      `TASK4760_STAMP_SEAL hand_written_draft_accepted=${anyoneConsentStampIsWhole(draft)}`
      + ` ticked_gate_stamp_accepted=${anyoneConsentStampIsWhole(turnedOn.stamp)}`,
    );
  });

  it("keeps the wording version tied to the wording that was shown", () => {
    const { store, gate } = gateFrom("allowed");
    const ticked = setAnyoneConsentGateTick(gate, true, TICKED_AT);
    // A gate whose wording was swapped after the tick cannot cash in the stamp.
    const reworded: AnyoneConsentGate = {
      ...ticked,
      sentences: ["Anyone can find you.", ...ANYONE_CONSENT_SENTENCES.slice(1)],
    };
    const before = readDiscoverySetting(store);
    const refused = turnOnAnyoneFromGate(reworded, store);
    expect(refused).toEqual({ ok: false, refusal: ANYONE_NEEDS_GATE });
    expect(readDiscoverySetting(store)).toBe(before);
    console.log(
      `TASK4760_WORDING_VERSION shown=${wordingVersionFor(ANYONE_CONSENT_SENTENCES)}`
      + ` reworded=${wordingVersionFor(reworded.sentences)}`
      + ` refusal="${refused.ok ? "" : refused.refusal}" setting_before=${before} setting_after=${readDiscoverySetting(store)}`,
    );
  });

  it("refuses a stored anyone that has no consent stamp behind it", () => {
    const { store, gate } = gateFrom("allowed");
    const turnedOn = turnOnAnyoneFromGate(setAnyoneConsentGateTick(gate, true, TICKED_AT), store);
    if (!turnedOn.ok) throw new Error("the gate refused a properly ticked box");
    const whole = turnedOn.stamp;
    const tampered = createDiscoverySettingStore({ setting: "anyone" });
    const forged = createDiscoverySettingStore({
      setting: "anyone",
      consentStamp: { recordedDate: TICKED_AT, wordingVersion: ANYONE_CONSENT_WORDING_VERSION },
    });
    const real = createDiscoverySettingStore({ setting: "anyone", consentStamp: whole });
    expect(tampered.setting).toBe("never");
    expect(tampered.corrections).toEqual([`${ANYONE_NEEDS_GATE} (stored setting corrected to never)`]);
    expect(forged.setting).toBe("never");
    expect(real.setting).toBe("anyone");
    console.log(
      `TASK4760_RESTORE tampered=${tampered.setting} reason="${tampered.corrections[0]}"`
      + ` forged_stamp=${forged.setting} real_stamp=${real.setting}`,
    );
  });

  it("keeps the other three choices working without any gate", () => {
    const store = createDiscoverySettingStore();
    const moves: string[] = [`fresh=${readDiscoverySetting(store)}`];
    for (const value of ["allowed", "shared-room", "never"] as const) {
      const result = chooseDiscoverySetting(store, value);
      expect(result).toEqual({ ok: true, setting: value });
      moves.push(`${value}=${readDiscoverySetting(store)}`);
    }
    const fifth = chooseDiscoverySetting(store, "fifth");
    expect(fifth).toEqual({ ok: false, refusal: "unknown discovery setting fifth" });
    console.log(`TASK4760_OTHER_CHOICES ${moves.join(" ")} fifth="${fifth.ok ? "" : fifth.refusal}"`);
  });

  it("proves exactly one code path can move the stored setting to anyone", () => {
    const exports = Object.entries(gateModule).filter(([, value]) => typeof value === "function");

    /**
     * Calls one export every way it could plausibly be called, aiming each call
     * at `anyone`, and says whether any store it touched came back on `anyone`.
     */
    function sweep(name: string, fn: unknown, stampShapes: readonly unknown[]): { moved: boolean; calls: number } {
      const call = fn as (...args: unknown[]) => unknown;
      let moved = false;
      let calls = 0;
      for (const stamp of stampShapes) {
        const firsts = [
          () => createDiscoverySettingStore({ setting: "allowed" }),
          () => openAnyoneConsentGate(createDiscoverySettingStore({ setting: "allowed" })),
          () => ({ ...openAnyoneConsentGate(createDiscoverySettingStore({ setting: "allowed" })), ticked: true, stamp }),
          () => ({ setting: "anyone", consentStamp: stamp }),
          () => "anyone",
          () => ANYONE_CONSENT_SENTENCES,
        ];
        for (const first of firsts) {
          const store = createDiscoverySettingStore({ setting: "allowed" });
          const attempts: unknown[][] = [
            [first(), store, stamp],
            [first(), "anyone", store],
            [first(), true, TICKED_AT],
            [first()],
          ];
          for (const args of attempts) {
            calls += 1;
            const touched = args.filter((arg): arg is DiscoverySettingStore =>
              typeof arg === "object" && arg !== null && "corrections" in arg);
            try {
              const returned = call(...args);
              if (typeof returned === "object" && returned !== null && "corrections" in returned) {
                touched.push(returned as DiscoverySettingStore);
              }
            } catch {
              // A throw is a refusal, which is what this sweep is looking for.
            }
            if (touched.some((candidate) => candidate.setting === "anyone")) moved = true;
          }
        }
      }
      void name;
      return { moved, calls };
    }

    // --- with no consent stamp in hand: nothing may reach anyone ------------
    const forged = [
      undefined,
      null,
      "anyone",
      { recordedDate: TICKED_AT, wordingVersion: ANYONE_CONSENT_WORDING_VERSION },
      { recordedDate: TICKED_AT, wordingVersion: ANYONE_CONSENT_WORDING_VERSION, seal: "deadbeef" },
      { recordedDate: TICKED_AT },
      { wordingVersion: ANYONE_CONSENT_WORDING_VERSION },
      recordAnyoneConsentStamp({ recordedDate: TICKED_AT, wordingVersion: ANYONE_CONSENT_WORDING_VERSION }),
    ];
    const withoutGate: string[] = [];
    let callsWithout = 0;
    for (const [name, fn] of exports) {
      const result = sweep(name, fn, forged);
      callsWithout += result.calls;
      if (result.moved) withoutGate.push(name);
    }
    expect(withoutGate).toEqual([]);
    console.log(
      `TASK4760_SWEEP_WITHOUT_GATE exports_called=${exports.length} calls=${callsWithout}`
      + ` reached_anyone=${withoutGate.length}`,
    );

    // --- with the stamp a ticked gate issued -------------------------------
    const issuedFrom = gateFrom("allowed");
    const issuing = turnOnAnyoneFromGate(setAnyoneConsentGateTick(issuedFrom.gate, true, TICKED_AT), issuedFrom.store);
    if (!issuing.ok) throw new Error("the gate refused a properly ticked box");
    expect(issuedFrom.store.setting).toBe("anyone");
    const real = [issuing.stamp];
    const withGate: string[] = [];
    let callsWith = 0;
    for (const [name, fn] of exports) {
      const result = sweep(name, fn, real);
      callsWith += result.calls;
      if (result.moved) withGate.push(name);
    }
    // `createDiscoverySettingStore` only reads back a stamp the gate already
    // issued, it cannot make one; `turnOnAnyoneFromGate` is the one mover.
    expect(withGate.sort()).toEqual(["createDiscoverySettingStore", "turnOnAnyoneFromGate"]);
    const movers = withGate.filter((name) => name !== "createDiscoverySettingStore");
    expect(movers).toEqual(["turnOnAnyoneFromGate"]);
    console.log(
      `TASK4760_SWEEP_WITH_GATE exports_called=${exports.length} calls=${callsWith}`
      + ` reached_anyone=${withGate.length} movers=${movers.length} via=${movers.join(",")}`
      + ` reflectors=${withGate.length - movers.length} via=createDiscoverySettingStore`,
    );
  });
});
