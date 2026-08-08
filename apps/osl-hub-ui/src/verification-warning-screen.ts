import "./verification-warning-screen.css";
import { choiceRadio } from "./onboarding-controls";
import type { VerificationWarningSetting } from "./verification-warning";

/**
 * The screen where the saved verification warning choice is picked.
 *
 * The four labels are the EXACT strings the backend stores
 * (`crates/ipc/src/app_preferences.rs::VerificationWarningChoice::label`), so
 * what the screen shows is what lands in `app_preferences.json`. Do not
 * sentence-case them here: a "Before sending" on screen and a "before sending"
 * on disk is the drift this screen exists to avoid.
 */
export const VERIFICATION_WARNING_CHOICES = ["every time", "once", "before sending", "never"] as const;

export type VerificationWarningChoice = typeof VERIFICATION_WARNING_CHOICES[number];

/** Matches the backend `#[default] EveryTime`. Reset returns here. */
export const DEFAULT_VERIFICATION_WARNING_CHOICE: VerificationWarningChoice = "every time";

export interface VerificationWarningScreenState {
  /** The choice currently lit on screen. */
  selected: VerificationWarningChoice;
  /** The choice the backend last accepted. */
  saved: VerificationWarningChoice;
}

/** Bridges the screen labels to the decision engine's setting ids. */
const SETTING_BY_CHOICE: Record<VerificationWarningChoice, VerificationWarningSetting> = {
  "every time": "every-time",
  once: "once",
  "before sending": "before-send",
  never: "never",
};

export function verificationWarningSettingFor(choice: VerificationWarningChoice): VerificationWarningSetting {
  return SETTING_BY_CHOICE[choice];
}

/**
 * One plain sentence for what happens, then one for what it costs. Both halves
 * are required: a screen that only says "you get fewer reminders" hides the
 * trade-off, and a screen that only warns about the risk reads as a scolding.
 * No word here names a mechanism -- "checked" is the whole vocabulary.
 */
const EFFECT_TEXT: Record<VerificationWarningChoice, string> = {
  "every time": "OSL reminds you every time you open a chat with someone you have not checked yet. Most reminders, and the least chance of missing one.",
  once: "OSL reminds you the first time you open a chat with someone you have not checked yet, then leaves that chat alone. Fewer reminders, and you still hear it once.",
  "before sending": "OSL reminds you just before you send to someone you have not checked yet. Nothing interrupts your reading, and you still get a nudge before you speak.",
  never: "OSL never reminds you. Nothing interrupts you, and nobody tells you when the person you are talking to has not been checked.",
};

export function verificationWarningEffectText(choice: VerificationWarningChoice): string {
  return EFFECT_TEXT[choice];
}

const OPTION_WORD_KEY: Record<VerificationWarningChoice, string> = {
  "every time": "every_time",
  once: "once",
  "before sending": "before_sending",
  never: "never",
};

function requireWord(words: Record<string, string>, key: string): string {
  const value = words[key];
  if (!value) throw new Error(`OSL: missing verification warning screen word '${key}'`);
  return value;
}

export function parseVerificationWarningChoice(input: string): VerificationWarningChoice | null {
  const normalized = input.trim().toLowerCase().replace(/[-_]+/gu, " ").replace(/\s+/gu, " ");
  return VERIFICATION_WARNING_CHOICES.find((choice) => choice === normalized) ?? null;
}

export interface VerificationWarningOption {
  choice: VerificationWarningChoice;
  label: string;
  effect: string;
}

export function verificationWarningOptions(words: Record<string, string>): VerificationWarningOption[] {
  return VERIFICATION_WARNING_CHOICES.map((choice) => {
    const key = OPTION_WORD_KEY[choice];
    return {
      choice,
      label: requireWord(words, `option_${key}_label`),
      effect: requireWord(words, `option_${key}_effect`),
    };
  });
}

export function verificationWarningEffect(
  choice: VerificationWarningChoice,
  words: Record<string, string>,
): string {
  return requireWord(words, `option_${OPTION_WORD_KEY[choice]}_effect`);
}

export function initialVerificationWarningScreenState(
  saved: VerificationWarningChoice = DEFAULT_VERIFICATION_WARNING_CHOICE,
): VerificationWarningScreenState {
  return { selected: saved, saved };
}

/** Picking a choice REPLACES the selection; it does not save it. */
export function chooseVerificationWarning(
  state: VerificationWarningScreenState,
  choice: VerificationWarningChoice,
): VerificationWarningScreenState {
  return { ...state, selected: choice };
}

/** The save control. Only this makes the selection the saved choice. */
export function saveVerificationWarningChoice(
  state: VerificationWarningScreenState,
): VerificationWarningScreenState {
  return { selected: state.selected, saved: state.selected };
}

/**
 * The reset control. It puts the DEFAULT back on screen -- it does not undo to
 * whatever was saved, because a user who wants the safe setting back should not
 * have to remember what they had. It stays unsaved until save is pressed.
 */
export function resetVerificationWarningChoice(
  state: VerificationWarningScreenState,
): VerificationWarningScreenState {
  return { ...state, selected: DEFAULT_VERIFICATION_WARNING_CHOICE };
}

export function isVerificationWarningSaved(state: VerificationWarningScreenState): boolean {
  return state.selected === state.saved;
}

export function verificationWarningHasUnsavedChange(state: VerificationWarningScreenState): boolean {
  return !isVerificationWarningSaved(state);
}

const choiceId = (choice: VerificationWarningChoice): string => `verification-warning-${choice.replace(/ /gu, "-")}`;

export function verificationWarningScreenMarkup(
  state: VerificationWarningScreenState,
  words?: Record<string, string>,
): string {
  if (words) {
    const dirty = verificationWarningHasUnsavedChange(state);
    const options = verificationWarningOptions(words).map((option) => {
      const selected = state.selected === option.choice;
      return `<label class="setting-option vw-option${selected ? " selected" : ""}">
        <input class="sr-only" type="radio" name="verification-warning" value="${option.choice}"${selected ? " checked" : ""}/>
        <span class="vw-option-head">${choiceRadio()}<strong>${option.label}</strong></span>
        <small class="vw-option-effect">${option.effect}</small>
      </label>`;
    }).join("");
    const stateTemplate = requireWord(words, dirty ? "state_unsaved_template" : "state_saved_template");
    return `<section class="settings-section verification-warning-screen" aria-labelledby="verification-warning-heading">
      <h2 id="verification-warning-heading" tabindex="-1">${requireWord(words, "heading")}</h2>
      <p class="vw-intro">${requireWord(words, "intro")}</p>
      <fieldset class="settings-options vw-options"><legend class="sr-only">${requireWord(words, "legend_label")}</legend>${options}</fieldset>
      <p class="vw-effect" data-verification-warning-effect><span class="vw-effect-label">${requireWord(words, "what_this_does_label")}</span>${verificationWarningEffect(state.selected, words)}</p>
      <div class="settings-actions vw-actions">
        <button class="button ghost vw-reset" type="button" data-verification-warning-reset${dirty ? "" : " disabled"}>${requireWord(words, "reset_button")}</button>
        <button class="button vw-save" type="button" data-verification-warning-save${dirty ? "" : " disabled"}>${requireWord(words, "save_button")}</button>
      </div>
      <p class="vw-state" data-verification-warning-state>${stateTemplate.replace("{choice}", state.saved)}</p>
    </section>`;
  }
  const choices = VERIFICATION_WARNING_CHOICES.map((choice) => {
    const selected = state.selected === choice;
    return `<label class="vw-choice${selected ? " selected" : ""}" for="${choiceId(choice)}">
      <input class="sr-only" id="${choiceId(choice)}" type="radio" name="verification-warning" value="${choice}" aria-label="${choice}"${selected ? " checked" : ""}/>
      <span class="vw-choice-head">${choiceRadio()}<strong>${choice}</strong></span>
    </label>`;
  }).join("");

  const saved = isVerificationWarningSaved(state);
  return `<section class="verification-warning-screen" aria-labelledby="verification-warning-title">
    <h1 id="verification-warning-title" tabindex="-1" class="vw-title">Verification warning</h1>
    <p class="vw-lead">Until you have checked someone, you cannot be sure the person on the other end is who you think. Choose when OSL should remind you.</p>
    <fieldset class="vw-choices"><legend class="sr-only">Remind me</legend>${choices}</fieldset>
    <p class="vw-effect" data-vw-effect>${verificationWarningEffectText(state.selected)}</p>
    <div class="vw-actions">
      <button class="button primary vw-save" data-vw-save type="button">Save</button>
      <button class="button ghost vw-reset" data-vw-reset type="button">Reset</button>
    </div>
    <p class="vw-status" data-vw-status data-saved="${saved}">${saved ? `Saved choice: ${state.saved}` : `Not saved yet. Saved choice is still ${state.saved}.`}</p>
  </section>`;
}

export interface VerificationWarningScreenHandle {
  state(): VerificationWarningScreenState;
}

export interface MountVerificationWarningScreenOptions {
  state?: VerificationWarningScreenState;
  /** Receives the exact backend label to persist. */
  onSave?: (choice: VerificationWarningChoice) => void;
}

/**
 * Redraws the whole section on every change. The screen is four radios and two
 * buttons; a diffing pass here would cost more to read than it saves to run.
 */
export function mountVerificationWarningScreen(
  root: HTMLElement,
  { state = initialVerificationWarningScreenState(), onSave }: MountVerificationWarningScreenOptions = {},
): VerificationWarningScreenHandle {
  let current = state;

  const draw = (): void => {
    root.innerHTML = verificationWarningScreenMarkup(current);
  };

  root.addEventListener("change", (event) => {
    const input = event.target as HTMLInputElement | null;
    if (!input || input.name !== "verification-warning") return;
    current = chooseVerificationWarning(current, input.value as VerificationWarningChoice);
    draw();
  });

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("[data-vw-save]")) {
      current = saveVerificationWarningChoice(current);
      onSave?.(current.saved);
      draw();
      return;
    }
    if (target?.closest("[data-vw-reset]")) {
      current = resetVerificationWarningChoice(current);
      draw();
    }
  });

  draw();
  return { state: () => current };
}
