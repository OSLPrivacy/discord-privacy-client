/**
 * Being seen as an OSL user — the one discovery setting, and the read-only row
 * that used to be a two-state cycler.
 *
 * The four choices, their order, and their saved words are ruling A10 and the
 * backend setting built by TASK 4750 (`never`, `allowed`, `shared-room`,
 * `anyone`, in that order, defaulting to `never`). The words on screen are the
 * ruled labels; the words saved are the backend's, so a person picking choice 1
 * saves `never` and not a UI synonym of it.
 *
 * Two pieces of reference copy are deliberately absent and must stay absent:
 * the old first discovery radio at Settings.dc.html:385, whose explanation said
 * any OSL user in the same chat would see a mark, and the Strip's two-state
 * stranger-findability cycler at Strip.dc.html:825. The literal wording of both
 * is written down once, in the guard test that proves neither ships, and
 * nowhere else in the source.
 *
 * Both described a mark placed where a carrier could see it, which is the exact
 * thing this design forbids: a mark a stranger can recognise inside a carrier
 * message is a public OSL detector the carrier can run over its whole history.
 * Nothing about discovery is written into anything a carrier can see; the answer
 * lives on OSL's own key server as a blinded card.
 *
 * The cycler also could not ship for a second reason. "Anyone" is gated by a
 * consent screen (TASK 4760), and a two-state NO/YES control has nowhere to put
 * a gate — it flips on the click. So the Strip carries a read-only row instead:
 * it shows the choice in force and opens Settings, and it cannot write.
 */

export type DiscoveryChoiceId = "never" | "allowed" | "shared-room" | "anyone";

export interface DiscoveryChoice {
  readonly id: DiscoveryChoiceId;
  /** The word the backend stores for this choice (TASK 4750). */
  readonly savedWord: DiscoveryChoiceId;
  /** The label ruling A10 puts on screen, character for character. */
  readonly label: string;
  readonly explanation: string;
  /** True when picking this choice has to pass the consent gate first. */
  readonly needsConsentGate: boolean;
}

/**
 * Ruling A10's order. Nothing may reorder this list: the first entry is both the
 * safest choice and the shipped default, and a screen that sorted these any
 * other way would put "Anyone" where a person's eye lands first.
 */
export const DISCOVERY_CHOICES: readonly DiscoveryChoice[] = [
  {
    id: "never",
    savedWord: "never",
    label: "Never show me",
    explanation: "You look like an ordinary user everywhere. OSL publishes nothing about you.",
    needsConsentGate: false,
  },
  {
    id: "allowed",
    savedWord: "allowed",
    label: "Only people I've allowed",
    explanation: "Only contacts on your allowed list can find you. Strangers cannot.",
    needsConsentGate: false,
  },
  {
    id: "shared-room",
    savedWord: "shared-room",
    label: "Anyone I've shared a chat with",
    explanation: "Only matches when you have both been in the same chat and you both chose this. One side alone finds nothing.",
    needsConsentGate: false,
  },
  {
    id: "anyone",
    savedWord: "anyone",
    label: "Anyone",
    explanation: "Anyone who knows your handle can learn you run OSL. OSL asks you to confirm this before turning it on.",
    needsConsentGate: true,
  },
];

/** A fresh profile starts here (TASK 4750). */
export const DISCOVERY_DEFAULT_CHOICE: DiscoveryChoiceId = "never";

/**
 * The reply-to-discovery-pings switch, kept under the four choices. It is a
 * master off: with it off OSL publishes nothing and answers nothing, whatever
 * the four-way setting says. It starts off.
 */
export const DISCOVERY_PINGS_LABEL = "Reply to discovery pings";
export const DISCOVERY_PINGS_EXPLANATION = "Off means OSL never answers 'are you on OSL?'";
export const DISCOVERY_PINGS_DEFAULT = false;

/**
 * The one-line honest summary at the bottom of TASK 4762's watcher table, which
 * that task's finish line makes the disclosure sentence on this screen. It is
 * reproduced character for character; changing it here without rerunning 4762
 * would put a claim on screen that no run backs.
 */
export const DISCOVERY_DISCLOSURE_SENTENCE =
  "If you choose Anyone, a stranger holding your handle learns yes, and that cannot be un-learned.";

/** TASK 4760's refusal, word for word, so one wording covers every path. */
export const DISCOVERY_CONSENT_REFUSAL = "discovery: anyone needs the consent gate";

export const DISCOVERY_VISIBILITY_STORAGE_KEY = "osl.discovery-visibility";

export interface DiscoveryVisibilityState {
  readonly choice: DiscoveryChoiceId;
  readonly replyToPings: boolean;
}

export function defaultDiscoveryVisibilityState(): DiscoveryVisibilityState {
  return { choice: DISCOVERY_DEFAULT_CHOICE, replyToPings: DISCOVERY_PINGS_DEFAULT };
}

export function discoveryChoice(id: string): DiscoveryChoice | null {
  return DISCOVERY_CHOICES.find((choice) => choice.id === id) ?? null;
}

/** The four saved words, in A10's order — what a printer or a check reads. */
export function discoveryChoiceOrder(): DiscoveryChoiceId[] {
  return DISCOVERY_CHOICES.map((choice) => choice.id);
}

/** The four on-screen labels, in A10's order. */
export function discoveryChoiceLabels(): string[] {
  return DISCOVERY_CHOICES.map((choice) => choice.label);
}

export interface DiscoverySelectOptions {
  /** Proof that TASK 4760's consent gate was passed for this change. */
  readonly consentGatePassed?: boolean;
}

/**
 * A choice change, refused rather than guessed. An unknown word is refused with
 * the backend's own wording, and "Anyone" is refused unless the consent gate
 * says it was passed — the screen must not be the path that walks around it.
 */
export function selectDiscoveryChoice(
  state: DiscoveryVisibilityState,
  choiceId: string,
  options: DiscoverySelectOptions = {},
): DiscoveryVisibilityState {
  const choice = discoveryChoice(choiceId);
  if (!choice) throw new Error(`unknown discovery setting ${choiceId}`);
  if (choice.needsConsentGate && options.consentGatePassed !== true) {
    throw new Error(DISCOVERY_CONSENT_REFUSAL);
  }
  return { ...state, choice: choice.id };
}

export function setDiscoveryReplyToPings(
  state: DiscoveryVisibilityState,
  replyToPings: boolean,
): DiscoveryVisibilityState {
  return { ...state, replyToPings };
}

export function serializeDiscoveryVisibility(state: DiscoveryVisibilityState): string {
  return JSON.stringify({ choice: state.choice, replyToPings: state.replyToPings });
}

/**
 * Read back what was saved. Anything unreadable, and any stored word that is not
 * one of the four, falls back to `never`: a damaged file must not widen a
 * setting that was closed. `anyone` is not restored from storage either, because
 * a value written straight into the file never passed the consent gate.
 */
export function readSavedDiscoveryVisibility(raw: string | null): DiscoveryVisibilityState {
  const fallback = defaultDiscoveryVisibilityState();
  if (!raw) return fallback;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return fallback;
  }
  if (!parsed || typeof parsed !== "object") return fallback;
  const record = parsed as Record<string, unknown>;
  const stored = typeof record.choice === "string" ? discoveryChoice(record.choice) : null;
  const choice = stored && !stored.needsConsentGate ? stored.id : DISCOVERY_DEFAULT_CHOICE;
  return { choice, replyToPings: record.replyToPings === true };
}

export interface DiscoveryChoiceRow {
  readonly choice: DiscoveryChoice;
  readonly checked: boolean;
  /** 1-based, so "choice 1 is selected on a fresh profile" is readable. */
  readonly position: number;
}

/** One row per choice, in A10's order, each already carrying its checked state. */
export function discoveryChoiceRows(state: DiscoveryVisibilityState): DiscoveryChoiceRow[] {
  return DISCOVERY_CHOICES.map((choice, index) => ({
    choice,
    checked: choice.id === state.choice,
    position: index + 1,
  }));
}

export function discoverySelectedChoice(state: DiscoveryVisibilityState): DiscoveryChoice {
  return discoveryChoice(state.choice) ?? discoveryChoice(DISCOVERY_DEFAULT_CHOICE)!;
}

/**
 * What the Strip shows where the old two-state stranger-findability cycler used
 * to be. It is handed the whole store, so it plainly *could* write the setting; it
 * does not, and a run can count that rather than take it on trust.
 */
export interface DiscoveryVisibilityStore {
  read(): DiscoveryVisibilityState;
  write(next: DiscoveryVisibilityState): void;
}

export interface DiscoveryStripRow {
  readonly name: string;
  /** The choice in force, spelled the way the settings screen spells it. */
  readonly value: string;
  readonly readOnly: true;
  readonly opensSettingsSection: "discovery";
  /** Clicking the row. Opens Settings; never writes. */
  activate(): void;
}

export const DISCOVERY_STRIP_ROW_NAME = "Being seen as an OSL user";

export function discoveryStripRow(
  store: DiscoveryVisibilityStore,
  openSettings: () => void,
): DiscoveryStripRow {
  return {
    name: DISCOVERY_STRIP_ROW_NAME,
    value: discoverySelectedChoice(store.read()).label,
    readOnly: true,
    opensSettingsSection: "discovery",
    activate: () => {
      openSettings();
    },
  };
}
