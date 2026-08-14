// TASK 4760b - the three ways round the "Anyone" consent gate, and what stops
// each one.
//
// Task 4760 put a gate in front of the fourth discovery value. This module is
// the attack surface for that gate: it builds the three doors the task names,
// so each one can be walked up to and shown shut. It adds no new way in. Every
// door here funnels into `discovery-anyone-consent-gate.ts`, which still holds
// the ONE statement that can put `anyone` into the stored setting.
//
// The three doors:
//
//   1. THE SETTING-CHANGE PATH, called directly with `anyone` and no consent
//      stamp. That is `chooseDiscoverySetting()`, plus the gate's own
//      `turnOnAnyoneFromGate()` reached with a hand-built or forged stamp.
//   2. THE STORED SETTINGS FILE, edited by hand to say `anyone`, then a
//      restart. `restartDiscoveryFromSettingsFileText()` is the restart: it
//      parses the file, funnels it through the gate module's loader, and hands
//      back the corrected file text and the reason, so the caller can write the
//      correction to disk and log it.
//   3. THE OLD STRIP CYCLER -- the pre-4750 control that stepped the setting
//      through the four values on each click, with nothing in between. No copy
//      of it survives in this codebase (the check searches for one and finds
//      none), so a faithful replica is planted here, tagged
//      OSL-OLD-STRIP-CYCLER-REPLICA, purely so attempt 3 has something to
//      click. The replica does what a surviving copy would have to do: ask the
//      setting-change path for the next value. There is no other path for it to
//      use.
//
// And the consequence the finish line counts: a DISCOVERY CARD is the record
// that makes a person findable. The publisher below keys on the stored setting
// alone -- `anyone` publishes, everything else publishes nothing -- because the
// setting IS the thing that publishes you. So "0 cards published" is not a
// separate promise, it is the same fact read from the other end: a card can
// only appear if the setting reached `anyone`.

import {
  ANYONE_NEEDS_GATE,
  DISCOVERY_ANYONE,
  DISCOVERY_SETTING_CHOICES,
  anyoneConsentStampIsWhole,
  chooseDiscoverySetting,
  createDiscoverySettingStore,
  isDiscoverySetting,
  openAnyoneConsentGate,
  readDiscoverySetting,
  setAnyoneConsentGateTick,
  turnOnAnyoneFromGate,
} from "./discovery-anyone-consent-gate";
import type {
  AnyoneConsentGate,
  ChooseDiscoverySettingResult,
  DiscoverySetting,
  DiscoverySettingStore,
  PersistedDiscoverySetting,
} from "./discovery-anyone-consent-gate";
import "./discovery-anyone-walkaround.css";

export { ANYONE_NEEDS_GATE };

// ---------------------------------------------------------------------------
// Discovery cards: the consequence of the setting.
// ---------------------------------------------------------------------------

export interface DiscoveryCard {
  handle: string;
  publishedAt: string;
  /** Which wording the person consented to, or `none` if no stamp stood behind it. */
  wordingVersion: string;
}

export interface DiscoveryCardLedger {
  published: DiscoveryCard[];
  refusals: string[];
}

export function createDiscoveryCardLedger(): DiscoveryCardLedger {
  return { published: [], refusals: [] };
}

export type PublishDiscoveryCardResult =
  | { ok: true; card: DiscoveryCard }
  | { ok: false; refusal: string };

/**
 * Publishes the findable-by-anyone card, if the stored setting says to.
 *
 * The setting is the only thing consulted, on purpose. If a walkaround ever did
 * move the setting to `anyone`, a card would be published and the count would
 * stop being 0 -- which is exactly the signal the check watches. Checking the
 * stamp here too would hide a breach behind a second guard.
 */
export function publishDiscoveryCard(
  store: DiscoverySettingStore,
  ledger: DiscoveryCardLedger,
  handle: string,
  at: string,
): PublishDiscoveryCardResult {
  const setting = readDiscoverySetting(store);
  if (setting !== DISCOVERY_ANYONE) {
    const refusal = `discovery: no card is published while the setting is ${setting}`;
    ledger.refusals.push(refusal);
    return { ok: false, refusal };
  }
  const stamp = store.consentStamp;
  const card: DiscoveryCard = {
    handle,
    publishedAt: at,
    wordingVersion: anyoneConsentStampIsWhole(stamp) ? stamp.wordingVersion : "none",
  };
  ledger.published.push(card);
  return { ok: true, card };
}

export function publishedCardCount(ledger: DiscoveryCardLedger): number {
  return ledger.published.length;
}

// ---------------------------------------------------------------------------
// Door 1: the setting-change path, called directly.
// ---------------------------------------------------------------------------

export interface DirectAttemptOutcome {
  /** What was called. */
  probe: string;
  /** The words that call is supposed to be refused with. */
  expect: string;
  ok: boolean;
  refusal: string;
  settingBefore: DiscoverySetting;
  settingAfter: DiscoverySetting;
  cardsPublished: number;
}

/** A gate object built by hand, never opened on screen, carrying a forged stamp. */
function forgedGate(settingBefore: DiscoverySetting, stamp: unknown): AnyoneConsentGate {
  return {
    settingBefore,
    sentences: [
      "Anyone who knows your handle can learn you run OSL.",
      "That includes the carrier itself, which can check its whole history.",
      "This is retroactive and permanent. Turning it off later removes your record, but it does not un-tell anyone who already looked.",
    ],
    wordingVersion: "anyone-consent-v1+cdd02ec0",
    ticked: true,
    stamp: stamp as AnyoneConsentGate["stamp"],
    leftBy: null,
  };
}

/**
 * Every direct call this app exposes that names `anyone`, each with no consent
 * stamp behind it or with one that was not issued by a ticked gate.
 */
export interface DirectAnyoneProbe {
  probe: string;
  /**
   * The words this particular call is refused with. Asking the setting-change
   * path for `anyone` says `discovery: anyone needs the consent gate`. Reaching
   * the gate's own Turn on with a gate that was never ticked is a different
   * fault and says so; only once a stamp is present at all does the refusal
   * come back to the gate's words.
   */
  expect: string;
  call: (store: DiscoverySettingStore) => ChooseDiscoverySettingResult;
}

export function directAnyoneProbes(): DirectAnyoneProbe[] {
  const wholeShaped = {
    recordedDate: "2026-08-07T18:20:00.000Z",
    wordingVersion: "anyone-consent-v1+cdd02ec0",
  };
  return [
    {
      probe: "chooseDiscoverySetting(store, \"anyone\")",
      expect: ANYONE_NEEDS_GATE,
      call: (store) => chooseDiscoverySetting(store, DISCOVERY_ANYONE),
    },
    {
      probe: "chooseDiscoverySetting(store, \"anyone\") twice in a row",
      expect: ANYONE_NEEDS_GATE,
      call: (store) => {
        chooseDiscoverySetting(store, DISCOVERY_ANYONE);
        return chooseDiscoverySetting(store, DISCOVERY_ANYONE);
      },
    },
    {
      probe: "turnOnAnyoneFromGate(hand-built gate, no stamp)",
      expect: "discovery: tick the box before turning Anyone on",
      call: (store) => turnOnAnyoneFromGate(forgedGate(readDiscoverySetting(store), null), store),
    },
    {
      probe: "turnOnAnyoneFromGate(hand-built gate, unsealed stamp)",
      expect: ANYONE_NEEDS_GATE,
      call: (store) => turnOnAnyoneFromGate(forgedGate(readDiscoverySetting(store), wholeShaped), store),
    },
    {
      probe: "turnOnAnyoneFromGate(hand-built gate, forged seal)",
      expect: ANYONE_NEEDS_GATE,
      call: (store) =>
        turnOnAnyoneFromGate(forgedGate(readDiscoverySetting(store), { ...wholeShaped, seal: "deadbeef" }), store),
    },
    {
      probe: "turnOnAnyoneFromGate(hand-built gate, stamp copied from another date)",
      expect: ANYONE_NEEDS_GATE,
      call: (store) =>
        turnOnAnyoneFromGate(
          forgedGate(readDiscoverySetting(store), { ...wholeShaped, recordedDate: "1999-01-01T00:00:00.000Z", seal: "cdd02ec0" }),
          store,
        ),
    },
    {
      probe: "turnOnAnyoneFromGate(gate left by Back, then cashed in)",
      expect: ANYONE_NEEDS_GATE,
      call: (store) =>
        turnOnAnyoneFromGate(
          { ...forgedGate(readDiscoverySetting(store), { ...wholeShaped, seal: "deadbeef" }), leftBy: "back" },
          store,
        ),
    },
  ];
}

/** Runs one direct probe against a store on `startOn`, and counts the cards after. */
export function runDirectAnyoneProbe(
  probe: DirectAnyoneProbe,
  startOn: DiscoverySetting,
  at: string,
): DirectAttemptOutcome {
  const store = createDiscoverySettingStore({ setting: startOn });
  const ledger = createDiscoveryCardLedger();
  const settingBefore = readDiscoverySetting(store);
  const result = probe.call(store);
  const settingAfter = readDiscoverySetting(store);
  publishDiscoveryCard(store, ledger, "@someone", at);
  return {
    probe: probe.probe,
    expect: probe.expect,
    ok: result.ok,
    refusal: result.ok ? "" : result.refusal,
    settingBefore,
    settingAfter,
    cardsPublished: publishedCardCount(ledger),
  };
}

// ---------------------------------------------------------------------------
// Door 2: the stored settings file, edited by hand, then a restart.
// ---------------------------------------------------------------------------

export const DISCOVERY_SETTINGS_FILE_FORMAT = "osl-discovery-settings-v1";

export interface DiscoverySettingsFile {
  format: string;
  setting: unknown;
  consentStamp: unknown;
}

export function serialiseDiscoverySettingsFile(store: DiscoverySettingStore): string {
  const file: DiscoverySettingsFile = {
    format: DISCOVERY_SETTINGS_FILE_FORMAT,
    setting: readDiscoverySetting(store),
    consentStamp: store.consentStamp,
  };
  return `${JSON.stringify(file, null, 2)}\n`;
}

/** Tolerant on purpose: a file someone hand-edited into nonsense still restarts. */
export function parseDiscoverySettingsFile(text: string): PersistedDiscoverySetting {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text) as unknown;
  } catch {
    return { setting: undefined, consentStamp: undefined };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { setting: undefined, consentStamp: undefined };
  }
  const file = parsed as Partial<DiscoverySettingsFile>;
  return { setting: file.setting, consentStamp: file.consentStamp };
}

export interface DiscoveryRestart {
  store: DiscoverySettingStore;
  settingOnDiskBefore: unknown;
  settingAfterRestart: DiscoverySetting;
  /** The reasons the persisted value was not honoured. Empty when it was. */
  corrections: string[];
  /** The file text as it should now stand on disk. */
  correctedText: string;
  /** True when the file on disk has to be rewritten. */
  fileChanged: boolean;
  logLines: string[];
}

/**
 * The restart. Reads the file, funnels the persisted value through the gate
 * module's loader -- which is where the ONE `anyone` write lives -- and reports
 * the corrected file and the reason.
 *
 * A file that says `anyone` with no sealed stamp behind it comes back on
 * `never`, the safest of the four values. It is deliberately NOT put back to
 * whatever the file said before the edit: the file is the only record of that,
 * and the file is what was tampered with, so there is nothing trustworthy to go
 * back to.
 */
export function restartDiscoveryFromSettingsFileText(text: string, at: string): DiscoveryRestart {
  const persisted = parseDiscoverySettingsFile(text);
  const store = createDiscoverySettingStore(persisted);
  const correctedText = serialiseDiscoverySettingsFile(store);
  const corrections = [...store.corrections];
  return {
    store,
    settingOnDiskBefore: persisted.setting,
    settingAfterRestart: readDiscoverySetting(store),
    corrections,
    correctedText,
    fileChanged: correctedText !== text,
    logLines: corrections.map((reason) => `${at} discovery-settings-file: ${reason}`),
  };
}

// ---------------------------------------------------------------------------
// Door 3: the old Strip cycler. OSL-OLD-STRIP-CYCLER-REPLICA
// ---------------------------------------------------------------------------
//
// The pre-4750 control: one button in the settings strip, showing the current
// discovery value, stepping to the next of the four on every click and writing
// as it went. 4750 replaced it with the named four-way choice and 4760 put the
// gate in front of the fourth value. No copy of the old control survives; this
// replica exists so attempt 3 has a real button to click. It is tagged above so
// the check can tell a planted replica from a surviving copy.
//
// Note what the replica does NOT do: it does not touch `store.setting`. It
// cannot -- that statement is module-private to the gate. All it can do is what
// a surviving copy would have to do, which is ask the setting-change path for
// the next value, and be refused when the next value is `anyone`.

export const OLD_STRIP_CYCLER_ORDER: readonly DiscoverySetting[] = DISCOVERY_SETTING_CHOICES;

export const OLD_STRIP_CYCLER_LABEL = "Who can find me";

export function oldStripCyclerNextValue(current: DiscoverySetting): DiscoverySetting {
  const at = OLD_STRIP_CYCLER_ORDER.indexOf(current);
  return OLD_STRIP_CYCLER_ORDER[(at + 1) % OLD_STRIP_CYCLER_ORDER.length] as DiscoverySetting;
}

export interface StripCyclerClick {
  from: DiscoverySetting;
  asked: DiscoverySetting;
  ok: boolean;
  refusal: string;
  settingAfter: DiscoverySetting;
}

/** One click of the old cycler: step to the next value and try to write it. */
export function clickOldStripCycler(store: DiscoverySettingStore): StripCyclerClick {
  const from = readDiscoverySetting(store);
  const asked = oldStripCyclerNextValue(from);
  const result = chooseDiscoverySetting(store, asked);
  return {
    from,
    asked,
    ok: result.ok,
    refusal: result.ok ? "" : result.refusal,
    settingAfter: readDiscoverySetting(store),
  };
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

export function oldStripCyclerMarkup(store: DiscoverySettingStore, lastRefusal: string): string {
  const setting = readDiscoverySetting(store);
  return `<div class="old-strip-cycler" id="old-strip-cycler" data-setting="${escapeHtml(setting)}" data-next="${escapeHtml(oldStripCyclerNextValue(setting))}">
  <button class="button old-strip-cycler-button" id="old-strip-cycler-button" type="button">
    <span class="osc-label">${escapeHtml(OLD_STRIP_CYCLER_LABEL)}</span>
    <span class="osc-value" id="old-strip-cycler-value">${escapeHtml(setting)}</span>
  </button>
  <p class="osc-refusal" id="old-strip-cycler-refusal">${escapeHtml(lastRefusal)}</p>
</div>`;
}

export interface StripCyclerBinding {
  getStore(): DiscoverySettingStore;
  getLedger(): DiscoveryCardLedger;
  clicks(): StripCyclerClick[];
  render(): void;
}

/** Paints the replica into `root` and wires its one button. */
export function bindOldStripCycler(
  root: HTMLElement,
  store: DiscoverySettingStore,
  ledger: DiscoveryCardLedger,
  at: string,
): StripCyclerBinding {
  const clicks: StripCyclerClick[] = [];
  let lastRefusal = "";
  const render = () => {
    root.innerHTML = oldStripCyclerMarkup(store, lastRefusal);
  };
  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target?.closest?.("#old-strip-cycler-button")) return;
    const click = clickOldStripCycler(store);
    clicks.push(click);
    lastRefusal = click.refusal;
    // Whatever the click did or did not do, the publisher runs after it.
    publishDiscoveryCard(store, ledger, "@someone", at);
    render();
  });
  render();
  return { getStore: () => store, getLedger: () => ledger, clicks: () => [...clicks], render };
}

// ---------------------------------------------------------------------------
// Re-exports the check and the fixtures lean on.
// ---------------------------------------------------------------------------

export {
  DISCOVERY_ANYONE,
  DISCOVERY_SETTING_CHOICES,
  anyoneConsentStampIsWhole,
  chooseDiscoverySetting,
  createDiscoverySettingStore,
  isDiscoverySetting,
  openAnyoneConsentGate,
  readDiscoverySetting,
  setAnyoneConsentGateTick,
  turnOnAnyoneFromGate,
};
export type { DiscoverySetting, DiscoverySettingStore };

/**
 * The positive control for the card counter: a real gate, really ticked, really
 * turned on. If this did not publish a card, "0 cards published" everywhere
 * else would be decoration rather than a result.
 */
export function publishAfterRealConsent(startOn: DiscoverySetting, at: string): {
  settingBefore: DiscoverySetting;
  settingAfter: DiscoverySetting;
  cardsPublished: number;
  wordingVersion: string;
} {
  const store = createDiscoverySettingStore({ setting: startOn });
  const ledger = createDiscoveryCardLedger();
  const settingBefore = readDiscoverySetting(store);
  const refused = chooseDiscoverySetting(store, DISCOVERY_ANYONE);
  const gate = refused.ok ? openAnyoneConsentGate(store) : refused.gate ?? openAnyoneConsentGate(store);
  const turnedOn = turnOnAnyoneFromGate(setAnyoneConsentGateTick(gate, true, at), store);
  publishDiscoveryCard(store, ledger, "@someone", at);
  return {
    settingBefore,
    settingAfter: readDiscoverySetting(store),
    cardsPublished: publishedCardCount(ledger),
    wordingVersion: turnedOn.ok ? turnedOn.stamp.wordingVersion : "none",
  };
}
