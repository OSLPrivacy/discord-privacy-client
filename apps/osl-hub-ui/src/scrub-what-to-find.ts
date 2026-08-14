import "./scrub-what-to-find.css";
import { invoke } from "@tauri-apps/api/core";
import { continueButton } from "./onboarding-controls";

/**
 * TASK 1413 - the "What counts as a bad message" page of Scrub setup.
 *
 * It sits between the consent page (TASK 1408) and the ready-to-scan page
 * (TASK 1418), and its only job is to hand the backend a bad-message selection:
 * which finding rules to use, plus any private words the owner types in.
 *
 * The rule names below are the six choices TASK 1411 defined, spelled the way
 * the backend parses them. TASK 1414 proved the parser refuses an unknown rule
 * name outright ("OSL: unknown rule name 'mystery rule'"), so the screen sends
 * those exact words rather than an id it invents and translates on the way out.
 */
export type BadMessageRuleName =
  | "passwords and codes"
  | "personal details"
  | "money details"
  | "private words"
  | "private pictures";

/**
 * The sixth choice. It is a shortcut for the five finding rules, never a rule
 * in its own right: choosing it saves all five, so nothing downstream has to
 * know the word "everything" to work out what to look for.
 */
export const EVERYTHING_ABOVE = "everything above" as const;

export type BadMessageChoiceName = BadMessageRuleName | typeof EVERYTHING_ABOVE;

export interface BadMessageChoice {
  name: BadMessageChoiceName;
  /** Sentence case for the control; the saved rule name stays lower case. */
  label: string;
  /** TASK 1411's plain explanation, unchanged. */
  explanation: string;
}

export const BAD_MESSAGE_CHOICES: readonly BadMessageChoice[] = [
  {
    name: "passwords and codes",
    label: "Passwords and codes",
    explanation:
      "Login passwords, one-time codes, recovery phrases, PINs, or invite codes that could let someone into an account.",
  },
  {
    name: "personal details",
    label: "Personal details",
    explanation:
      "Names, addresses, phone numbers, locations, IDs, health details, or other facts that identify a person.",
  },
  {
    name: "money details",
    label: "Money details",
    explanation:
      "Card numbers, bank details, invoices, tax details, account balances, or payment information.",
  },
  {
    name: "private words",
    label: "Private words",
    explanation:
      "Words or names you add yourself, like a project name, nickname, or phrase you do not want left in messages.",
  },
  {
    name: "private pictures",
    label: "Private pictures",
    explanation:
      "Photos, screenshots, scans, or attachments that may show people, documents, rooms, screens, or other private things.",
  },
  {
    name: EVERYTHING_ABOVE,
    label: "Everything above",
    explanation: "Use all of these rules together.",
  },
] as const;

/** The five rules a run can actually be saved with. "Everything above" is not one of them. */
export const BAD_MESSAGE_FINDING_RULES: readonly BadMessageRuleName[] = BAD_MESSAGE_CHOICES
  .map(({ name }) => name)
  .filter((name): name is BadMessageRuleName => name !== EVERYTHING_ABOVE);

/** One typed word per line or comma. These two are the screen's own limits, not the parser's. */
export const MAX_PRIVATE_WORDS = 32;
export const MAX_PRIVATE_WORD_LENGTH = 80;

export const SAVE_BAD_MESSAGE_SELECTION_COMMAND = "save_bad_message_rule_selection";

export interface WhatToFindState {
  /** Ticked rules, always in BAD_MESSAGE_FINDING_RULES order so the saved list is stable. */
  rules: readonly BadMessageRuleName[];
  /** Raw contents of the private words box, exactly as typed. */
  privateWordsText: string;
}

export interface BadMessageSelectionRequest {
  runId: string;
  ruleNames: readonly BadMessageRuleName[];
  privateWords: readonly string[];
  /**
   * TASK 1412's ruling, carried on the request rather than assumed downstream:
   * a hit is something for the owner to look at, never a finding of fact.
   */
  matchTreatment: "possible_match";
}

export type WhatToFindBlock = "no-rule-chosen" | null;

export function initialWhatToFindState(): WhatToFindState {
  return { rules: [], privateWordsText: "" };
}

export function isBadMessageRuleName(value: string): value is BadMessageRuleName {
  return (BAD_MESSAGE_FINDING_RULES as readonly string[]).includes(value);
}

export function isBadMessageChoiceName(value: string): value is BadMessageChoiceName {
  return value === EVERYTHING_ABOVE || isBadMessageRuleName(value);
}

/** True once every finding rule is ticked, which is what "Everything above" means. */
export function everythingAboveChosen(state: WhatToFindState): boolean {
  return BAD_MESSAGE_FINDING_RULES.every((rule) => state.rules.includes(rule));
}

export function badMessageRuleChosen(state: WhatToFindState, rule: BadMessageRuleName): boolean {
  return state.rules.includes(rule);
}

/**
 * Ticking "Everything above" ticks all five; unticking it clears all five.
 * Ticking the five one by one leaves "Everything above" ticked as well, because
 * the two states are the same selection and drawing them differently would be a
 * lie about what is saved.
 */
export function chooseBadMessageChoice(
  state: WhatToFindState,
  choice: BadMessageChoiceName,
): WhatToFindState {
  if (choice === EVERYTHING_ABOVE) {
    return {
      ...state,
      rules: everythingAboveChosen(state) ? [] : [...BAD_MESSAGE_FINDING_RULES],
    };
  }
  const next = state.rules.includes(choice)
    ? state.rules.filter((rule) => rule !== choice)
    : [...state.rules, choice];
  return { ...state, rules: BAD_MESSAGE_FINDING_RULES.filter((rule) => next.includes(rule)) };
}

export function setPrivateWordsText(state: WhatToFindState, text: string): WhatToFindState {
  return { ...state, privateWordsText: text };
}

/**
 * Lines or commas separate words; blanks are dropped rather than sent.
 * TASK 1414 proved the backend refuses an empty private word ("OSL: empty
 * private word"), so a stray blank line must never reach it as a word.
 * Repeats are dropped case-insensitively, keeping the spelling typed first.
 */
export function parsePrivateWords(text: string): string[] {
  const words: string[] = [];
  const seen = new Set<string>();
  for (const part of text.split(/[\n,]/u)) {
    const word = part.trim().replace(/\s+/gu, " ");
    if (!word || word.length > MAX_PRIVATE_WORD_LENGTH) continue;
    const key = word.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    words.push(word);
    if (words.length === MAX_PRIVATE_WORDS) break;
  }
  return words;
}

/**
 * Continue needs at least one rule and nothing else. In particular it does NOT
 * demand a private word when the private words rule is ticked: an empty box
 * sends an empty list, which is a run with no custom words, not the empty word
 * the backend refuses.
 */
export function whatToFindBlock(state: WhatToFindState): WhatToFindBlock {
  return state.rules.length === 0 ? "no-rule-chosen" : null;
}

export function badMessageSelectionRequest(
  runId: string,
  state: WhatToFindState,
): BadMessageSelectionRequest {
  return {
    runId,
    ruleNames: BAD_MESSAGE_FINDING_RULES.filter((rule) => state.rules.includes(rule)),
    privateWords: parsePrivateWords(state.privateWordsText),
    matchTreatment: "possible_match",
  };
}

export interface BadMessageSelectionPort {
  save(request: BadMessageSelectionRequest): Promise<unknown>;
}

/** The shipping port. One command name, in one place, so it cannot drift per screen. */
export const tauriBadMessageSelectionPort: BadMessageSelectionPort = {
  save: (request) => invoke(SAVE_BAD_MESSAGE_SELECTION_COMMAND, { request }),
};

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function choiceMarkup(choice: BadMessageChoice, chosen: boolean): string {
  const everything = choice.name === EVERYTHING_ABOVE;
  return `<label class="setting-option wtf-choice${everything ? " wtf-choice-all" : ""}${chosen ? " selected" : ""}">
      <span class="wtf-choice-head"><input class="wtf-choice-box" type="checkbox" name="bad-message-rule" value="${escapeHtml(choice.name)}"${chosen ? " checked" : ""}/><strong>${escapeHtml(choice.label)}</strong></span>
      <small class="wtf-choice-explanation">${escapeHtml(choice.explanation)}</small>
    </label>`;
}

export function whatToFindMarkup(state: WhatToFindState): string {
  const blocked = whatToFindBlock(state);
  const words = parsePrivateWords(state.privateWordsText);
  const choices = BAD_MESSAGE_CHOICES
    .map((choice) => choiceMarkup(
      choice,
      choice.name === EVERYTHING_ABOVE ? everythingAboveChosen(state) : badMessageRuleChosen(state, choice.name),
    ))
    .join("");
  const stateLine = blocked === "no-rule-chosen"
    ? "Choose at least one thing to look for."
    : `${state.rules.length} of ${BAD_MESSAGE_FINDING_RULES.length} rules chosen · ${words.length} private ${words.length === 1 ? "word" : "words"}.`;

  return `<section class="what-to-find" data-what-to-find aria-labelledby="what-to-find-heading">
    <h1 id="what-to-find-heading" tabindex="-1">What counts as a bad message</h1>
    <p class="wtf-intro">Choose what OSL should look for while it reads your own messages. Anything it finds is a <strong>possible match</strong> for you to review, not proof of anything.</p>
    <fieldset class="settings-options wtf-choices"><legend class="sr-only">What counts as a bad message</legend>${choices}</fieldset>
    <label class="wtf-words" for="private-words"><span class="wtf-words-label">Private words</span><small class="wtf-words-hint">One per line, or separated by commas. Up to ${MAX_PRIVATE_WORDS}. These stay on this device.</small><textarea id="private-words" name="private-words" data-private-words rows="4" spellcheck="false" autocomplete="off" placeholder="project bluebird&#10;the cabin">${escapeHtml(state.privateWordsText)}</textarea></label>
    <p class="wtf-words-count" data-private-words-count>${words.length} private ${words.length === 1 ? "word" : "words"} will be saved.</p>
    <p class="wtf-state" data-what-to-find-state role="status">${stateLine}</p>
    <div class="setup-footer onboarding-actions wtf-actions">
      <button class="button ghost onboarding-back" type="button" data-what-to-find-back>Back</button>
      ${continueButton(`data-what-to-find-continue${blocked ? " disabled" : ""}`)}
    </div>
  </section>`;
}

export interface WhatToFindPageOptions {
  /** The Scrub run this selection belongs to; the backend keys the save on it. */
  runId: string;
  port: BadMessageSelectionPort;
  state?: WhatToFindState;
  onBack?: () => void;
  onContinue?: (saved: BadMessageSelectionRequest) => void;
  onError?: (error: unknown) => void;
}

export interface WhatToFindPageHandle {
  state(): WhatToFindState;
  /** Resolves once the save started by the last Continue has settled. */
  settled(): Promise<void>;
}

/**
 * Mounts the page and wires it to the save. Continue is the only thing that
 * writes: the tick boxes and the words box move screen state and nothing else,
 * so leaving the page without pressing Continue saves nothing.
 */
export function mountWhatToFindPage(
  root: HTMLElement,
  options: WhatToFindPageOptions,
): WhatToFindPageHandle {
  let state = options.state ?? initialWhatToFindState();
  let saving: Promise<void> = Promise.resolve();

  const render = (): void => {
    root.innerHTML = whatToFindMarkup(state);
  };

  root.addEventListener("change", (event) => {
    const target = event.target as HTMLElement | null;
    if (target instanceof HTMLInputElement && target.name === "bad-message-rule") {
      if (!isBadMessageChoiceName(target.value)) return;
      state = chooseBadMessageChoice(state, target.value);
      render();
      return;
    }
    if (target instanceof HTMLTextAreaElement && target.dataset.privateWords !== undefined) {
      state = setPrivateWordsText(state, target.value);
      render();
    }
  });

  root.addEventListener("input", (event) => {
    const target = event.target as HTMLElement | null;
    if (!(target instanceof HTMLTextAreaElement) || target.dataset.privateWords === undefined) return;
    state = setPrivateWordsText(state, target.value);
    const count = root.querySelector("[data-private-words-count]");
    const words = parsePrivateWords(state.privateWordsText);
    if (count) count.textContent = `${words.length} private ${words.length === 1 ? "word" : "words"} will be saved.`;
  });

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("[data-what-to-find-back]")) {
      options.onBack?.();
      return;
    }
    if (!target?.closest("[data-what-to-find-continue]")) return;
    if (whatToFindBlock(state)) return;

    const request = badMessageSelectionRequest(options.runId, state);
    const section = root.querySelector<HTMLElement>("[data-what-to-find]");
    if (section) section.dataset.whatToFindSave = "saving";
    saving = Promise.resolve(options.port.save(request)).then(
      () => {
        const saved = root.querySelector<HTMLElement>("[data-what-to-find]");
        if (saved) {
          saved.dataset.whatToFindSave = "saved";
          saved.dataset.whatToFindSavedRules = String(request.ruleNames.length);
        }
        const line = root.querySelector("[data-what-to-find-state]");
        if (line) {
          line.textContent = `Saved. OSL will look for ${request.ruleNames.length} ${request.ruleNames.length === 1 ? "thing" : "things"} and ${request.privateWords.length} private ${request.privateWords.length === 1 ? "word" : "words"}.`;
        }
        options.onContinue?.(request);
      },
      (error: unknown) => {
        const failed = root.querySelector<HTMLElement>("[data-what-to-find]");
        if (failed) failed.dataset.whatToFindSave = "failed";
        const line = failed?.querySelector("[data-what-to-find-state]");
        if (line) line.textContent = "OSL could not save these choices. Nothing was changed.";
        options.onError?.(error);
      },
    );
  });

  render();
  return { state: () => state, settled: () => saving };
}
