// TASK 4760 - the consent gate in front of "Anyone".
//
// Task 4750 built the stored discovery setting: four values in one order --
// `never`, `allowed`, `shared-room`, `anyone` -- with a fresh profile on
// `never`. This module is the screen side of the fourth value. Choosing
// "Anyone" writes nothing at all; it opens a gate that carries three sentences
// word for word, one tick box that starts unticked, and a Turn on button that
// stays greyed until the box is ticked. Ruling 5 keeps typed words for ERASE,
// so there is no typed word here.
//
// The shape of the thing:
//
//   * `chooseDiscoverySetting()` is the ordinary four-way choice. It writes the
//     other three values straight away and it *cannot* write `anyone`: asked
//     for `anyone` it writes nothing, refuses with `discovery: anyone needs the
//     consent gate`, and hands back an open gate.
//   * `setAnyoneConsentGateTick()` is the tick. Ticking records the consent
//     stamp -- the date it was ticked, and which version of the wording was on
//     screen when it was ticked. Unticking throws the stamp away.
//   * `turnOnAnyoneFromGate()` is the only way out of the gate that changes
//     anything. Leaving any other way (Back, Escape, the close cross, picking a
//     different choice, or just walking off) leaves the stored setting exactly
//     as it was.
//   * `moveDiscoverySettingToAnyone()` is module-private and holds the ONE
//     statement in this codebase that can put `anyone` into the stored setting.
//     It refuses any stamp that is not a real, sealed stamp. Restoring a
//     persisted profile funnels through that same statement, so a settings file
//     that says `anyone` with no stamp behind it loads as `never` and says why.
//
// Honest about the seal: a consent stamp is sealed against being hand-built by
// other code in this app, not against an attacker who can edit this file. It is
// a code-path guard, and the search in `scripts/task-4760-anyone-path-search.mjs`
// is what proves the path count.

import "./discovery-anyone-consent-gate.css";

/** Task 4750's four values, in task 4750's order. */
export const DISCOVERY_SETTING_CHOICES = ["never", "allowed", "shared-room", "anyone"] as const;

export type DiscoverySetting = (typeof DISCOVERY_SETTING_CHOICES)[number];

/** The gated value. Kept as a named constant so a search can find every use. */
export const DISCOVERY_ANYONE: DiscoverySetting = "anyone";

/**
 * The three sentences the gate carries, word for word. Nothing may reword,
 * shorten, join or reorder them: the consent stamp records a version computed
 * from these exact strings, so changing one changes the version.
 */
export const ANYONE_CONSENT_SENTENCES = [
  "Anyone who knows your handle can learn you run OSL.",
  "That includes the carrier itself, which can check its whole history.",
  "This is retroactive and permanent. Turning it off later removes your record, but it does not un-tell anyone who already looked.",
] as const;

/** The tick box. One box, no typed word (ruling 5). */
export const ANYONE_CONSENT_TICK_LABEL =
  "I have read all three sentences and I want to be findable by anyone.";

export const ANYONE_CONSENT_GATE_TITLE = "Turn on Anyone?";

/** What the four-way choice says when it is asked for `anyone` directly. */
export const ANYONE_NEEDS_GATE = "discovery: anyone needs the consent gate";

/** What a half-built consent stamp is refused with. */
export const CONSENT_STAMP_NEEDS_BOTH = "discovery consent stamp needs a date and a wording version";

/** What the gate says when Turn on is reached with the box unticked. */
export const TURN_ON_NEEDS_TICK = "discovery: tick the box before turning Anyone on";

function fnv1a(text: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

/**
 * Which version of the wording a person was shown. Computed from the sentences
 * themselves, so a stamp can never claim a version for wording it never saw.
 */
export function wordingVersionFor(sentences: readonly string[]): string {
  return `anyone-consent-v1+${fnv1a(sentences.join(""))}`;
}

/** The version of the wording that ships. Checked against the sentences in the tests. */
export const ANYONE_CONSENT_WORDING_VERSION = "anyone-consent-v1+cdd02ec0";

export interface AnyoneConsentStampDraft {
  /** The day the box was ticked, as an ISO date-time string. */
  recordedDate: string;
  /** Which version of the three sentences was on screen at that moment. */
  wordingVersion: string;
}

export interface AnyoneConsentStamp extends AnyoneConsentStampDraft {
  /** Marks the stamp as one a ticked gate issued. Only ticking adds this. */
  seal: string;
}

const STAMP_SEAL_TAG = "osl-discovery-anyone-consent-stamp/1";

function sealFor(recordedDate: string, wordingVersion: string): string {
  return fnv1a(`${STAMP_SEAL_TAG}|${recordedDate}|${wordingVersion}`);
}

function isRealDate(value: unknown): value is string {
  return typeof value === "string"
    && /^\d{4}-\d{2}-\d{2}/u.test(value)
    && Number.isFinite(Date.parse(value));
}

function isRealWordingVersion(value: unknown): value is string {
  return typeof value === "string" && /^anyone-consent-v\d+\+[0-9a-f]{8}$/u.test(value);
}

export interface AnyoneConsentStampInput {
  recordedDate?: string | Date | null;
  wordingVersion?: string | null;
}

/**
 * Writes a consent stamp. Both halves are required: a stamp with no date, or
 * with no wording version, or with either one unreadable, is refused outright
 * rather than written half-filled.
 *
 * What comes back is a draft. Only ticking the box on an open gate seals one,
 * so no other caller can hand itself a stamp the gate never issued.
 */
export function recordAnyoneConsentStamp(input: AnyoneConsentStampInput): AnyoneConsentStampDraft {
  const raw = input.recordedDate instanceof Date
    ? (Number.isFinite(input.recordedDate.getTime()) ? input.recordedDate.toISOString() : "")
    : input.recordedDate;
  const recordedDate = typeof raw === "string" ? raw.trim() : "";
  const wordingVersion = typeof input.wordingVersion === "string" ? input.wordingVersion.trim() : "";
  if (!isRealDate(recordedDate) || !isRealWordingVersion(wordingVersion)) {
    throw new Error(CONSENT_STAMP_NEEDS_BOTH);
  }
  return { recordedDate, wordingVersion };
}

/** Module-private. Ticking the box is the only thing that calls it. */
function sealConsentStamp(draft: AnyoneConsentStampDraft): AnyoneConsentStamp {
  return { ...draft, seal: sealFor(draft.recordedDate, draft.wordingVersion) };
}

/** True only for a whole stamp this module issued. */
export function anyoneConsentStampIsWhole(stamp: unknown): stamp is AnyoneConsentStamp {
  if (typeof stamp !== "object" || stamp === null) return false;
  const candidate = stamp as Partial<AnyoneConsentStamp>;
  if (!isRealDate(candidate.recordedDate) || !isRealWordingVersion(candidate.wordingVersion)) return false;
  return candidate.seal === sealFor(candidate.recordedDate, candidate.wordingVersion);
}

export interface DiscoverySettingStore {
  setting: DiscoverySetting;
  consentStamp: AnyoneConsentStamp | null;
  /** Why a persisted value was not honoured, if it was not. */
  corrections: string[];
}

/**
 * THE ONE PATH. This is the only statement in the app that can put `anyone`
 * into the stored setting, and it refuses without a whole consent stamp.
 */
function moveDiscoverySettingToAnyone(
  store: DiscoverySettingStore,
  stamp: unknown,
): { ok: true; setting: DiscoverySetting } | { ok: false; refusal: string } {
  if (!anyoneConsentStampIsWhole(stamp)) return { ok: false, refusal: ANYONE_NEEDS_GATE };
  store.setting = DISCOVERY_ANYONE;
  store.consentStamp = stamp;
  return { ok: true, setting: store.setting };
}

export function isDiscoverySetting(value: unknown): value is DiscoverySetting {
  return typeof value === "string" && (DISCOVERY_SETTING_CHOICES as readonly string[]).includes(value);
}

export interface PersistedDiscoverySetting {
  setting?: unknown;
  consentStamp?: unknown;
}

/**
 * Loads the stored setting. A persisted `anyone` is only honoured when a whole
 * consent stamp sits beside it, and that goes through the one path above;
 * anything else lands on `never` and the reason is kept.
 */
export function createDiscoverySettingStore(persisted: PersistedDiscoverySetting = {}): DiscoverySettingStore {
  const store: DiscoverySettingStore = { setting: "never", consentStamp: null, corrections: [] };
  const wanted = persisted.setting;
  if (wanted === undefined || wanted === null) return store;
  if (!isDiscoverySetting(wanted)) {
    store.corrections.push(`unknown discovery setting ${String(wanted)}`);
    return store;
  }
  if (wanted === DISCOVERY_ANYONE) {
    const restored = moveDiscoverySettingToAnyone(store, persisted.consentStamp);
    if (!restored.ok) store.corrections.push(`${restored.refusal} (stored setting corrected to never)`);
    return store;
  }
  store.setting = wanted;
  return store;
}

export function readDiscoverySetting(store: DiscoverySettingStore): DiscoverySetting {
  return store.setting;
}

export interface AnyoneConsentGate {
  /** The value the setting had when the gate opened. */
  settingBefore: DiscoverySetting;
  sentences: readonly string[];
  wordingVersion: string;
  ticked: boolean;
  /** Written when the box is ticked, thrown away when it is unticked. */
  stamp: AnyoneConsentStamp | null;
  /** Set once the gate has been left; a left gate can no longer turn anything on. */
  leftBy: string | null;
}

export function openAnyoneConsentGate(store: DiscoverySettingStore): AnyoneConsentGate {
  return {
    settingBefore: store.setting,
    sentences: [...ANYONE_CONSENT_SENTENCES],
    wordingVersion: wordingVersionFor(ANYONE_CONSENT_SENTENCES),
    ticked: false,
    stamp: null,
    leftBy: null,
  };
}

export type ChooseDiscoverySettingResult =
  | { ok: true; setting: DiscoverySetting }
  | { ok: false; refusal: string; gate?: AnyoneConsentGate };

/**
 * The ordinary four-way choice. `anyone` is not one of the values it can
 * write: it opens the gate instead and the stored setting does not move.
 */
export function chooseDiscoverySetting(store: DiscoverySettingStore, value: string): ChooseDiscoverySettingResult {
  if (!isDiscoverySetting(value)) return { ok: false, refusal: `unknown discovery setting ${value}` };
  if (value === DISCOVERY_ANYONE) {
    return { ok: false, refusal: ANYONE_NEEDS_GATE, gate: openAnyoneConsentGate(store) };
  }
  store.setting = value;
  store.consentStamp = null;
  return { ok: true, setting: store.setting };
}

/**
 * The tick. Ticking records the consent stamp for the wording that is on
 * screen at that moment; unticking throws it away, so a stamp can never
 * outlive the tick that made it.
 */
export function setAnyoneConsentGateTick(
  gate: AnyoneConsentGate,
  ticked: boolean,
  now: Date | string = new Date(),
): AnyoneConsentGate {
  if (gate.leftBy !== null) return gate;
  if (!ticked) return { ...gate, ticked: false, stamp: null };
  const stamp = sealConsentStamp(recordAnyoneConsentStamp({
    recordedDate: now,
    wordingVersion: wordingVersionFor(gate.sentences),
  }));
  return { ...gate, ticked: true, stamp };
}

/** Turn on is greyed until the box is ticked. Nothing else moves it. */
export function anyoneConsentTurnOnAvailable(gate: AnyoneConsentGate): boolean {
  return gate.leftBy === null && gate.ticked && anyoneConsentStampIsWhole(gate.stamp);
}

/**
 * Every other way out: Back, Escape, the close cross, picking another choice.
 * It touches no store, so the stored setting stays on whatever it was.
 */
export function leaveAnyoneConsentGate(gate: AnyoneConsentGate, how: string): AnyoneConsentGate {
  return { ...gate, ticked: false, stamp: null, leftBy: how };
}

export type TurnOnAnyoneResult =
  | { ok: true; setting: DiscoverySetting; stamp: AnyoneConsentStamp }
  | { ok: false; refusal: string };

/**
 * The only way off this gate that changes anything. It re-checks the tick
 * itself, so calling it without ever pressing the button is refused the same
 * way the greyed button refuses.
 */
export function turnOnAnyoneFromGate(
  gate: AnyoneConsentGate,
  store: DiscoverySettingStore,
): TurnOnAnyoneResult {
  if (gate.leftBy !== null) return { ok: false, refusal: ANYONE_NEEDS_GATE };
  if (!gate.ticked || gate.stamp === null) return { ok: false, refusal: TURN_ON_NEEDS_TICK };
  // The stamp has to be for the wording that is actually on screen.
  if (gate.stamp.wordingVersion !== wordingVersionFor(gate.sentences)) {
    return { ok: false, refusal: ANYONE_NEEDS_GATE };
  }
  const moved = moveDiscoverySettingToAnyone(store, gate.stamp);
  if (!moved.ok) return moved;
  return { ok: true, setting: moved.setting, stamp: gate.stamp };
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

export function anyoneConsentGateMarkup(gate: AnyoneConsentGate): string {
  const available = anyoneConsentTurnOnAvailable(gate);
  const sentences = gate.sentences
    .map((sentence, index) =>
      `<p class="dag-sentence" id="discovery-anyone-sentence-${index}" data-sentence-index="${index}">${escapeHtml(sentence)}</p>`)
    .join("");
  return `<section class="discovery-anyone-gate" id="discovery-anyone-gate" role="dialog" aria-modal="true" aria-labelledby="discovery-anyone-gate-title" data-turn-on-available="${available ? "true" : "false"}" data-wording-version="${escapeHtml(gate.wordingVersion)}" data-setting-before="${escapeHtml(gate.settingBefore)}">
  <header class="dag-header">
    <h1 id="discovery-anyone-gate-title">${escapeHtml(ANYONE_CONSENT_GATE_TITLE)}</h1>
    <button class="button ghost dag-close" id="discovery-anyone-gate-close" type="button" aria-label="Close">Close</button>
  </header>
  <div class="dag-sentences" id="discovery-anyone-sentences">${sentences}</div>
  <label class="dag-tick" for="discovery-anyone-consent-tick"><input type="checkbox" id="discovery-anyone-consent-tick" data-consent-tick="anyone"${gate.ticked ? " checked" : ""}> ${escapeHtml(ANYONE_CONSENT_TICK_LABEL)}</label>
  <footer class="dag-footer">
    <button class="button ghost dag-back" id="discovery-anyone-gate-back" type="button">Back</button>
    <button class="button primary dag-turn-on" id="discovery-anyone-gate-turn-on" type="button"${available ? "" : ' disabled aria-disabled="true"'}>Turn on</button>
  </footer>
</section>`;
}

export interface AnyoneConsentGateBinding {
  getGate(): AnyoneConsentGate;
  getStore(): DiscoverySettingStore;
  render(): void;
  /** The direct call: no button press, same gate. */
  turnOnNow(): TurnOnAnyoneResult;
  /** Leaving any other way. */
  leave(how: string): AnyoneConsentGate;
}

export interface AnyoneConsentGateDeps {
  store: DiscoverySettingStore;
  now?: () => Date;
  onLeft?: (how: string, gate: AnyoneConsentGate) => void;
  onTurnedOn?: (result: TurnOnAnyoneResult) => void;
}

/** Paints the gate into `root` and wires the tick, Back, Close and Turn on. */
export function bindAnyoneConsentGate(
  root: HTMLElement,
  initial: AnyoneConsentGate,
  deps: AnyoneConsentGateDeps,
): AnyoneConsentGateBinding {
  let gate = initial;
  const render = () => {
    root.innerHTML = anyoneConsentGateMarkup(gate);
  };

  const leave = (how: string): AnyoneConsentGate => {
    gate = leaveAnyoneConsentGate(gate, how);
    render();
    deps.onLeft?.(how, gate);
    return gate;
  };

  const turnOnNow = (): TurnOnAnyoneResult => {
    const result = turnOnAnyoneFromGate(gate, deps.store);
    deps.onTurnedOn?.(result);
    return result;
  };

  root.addEventListener("change", (event) => {
    const target = event.target as HTMLInputElement | null;
    if (!target?.getAttribute?.("data-consent-tick")) return;
    gate = setAnyoneConsentGateTick(gate, target.checked, deps.now?.() ?? new Date());
    render();
  });

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest?.("#discovery-anyone-gate-back")) {
      leave("back");
      return;
    }
    if (target?.closest?.("#discovery-anyone-gate-close")) {
      leave("close");
      return;
    }
    if (target?.closest?.("#discovery-anyone-gate-turn-on")) turnOnNow();
  });

  root.addEventListener("keydown", (event) => {
    if ((event as KeyboardEvent).key === "Escape") leave("escape");
  });

  render();
  return { getGate: () => gate, getStore: () => deps.store, render, turnOnNow, leave };
}
