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

const choiceId = (choice: VerificationWarningChoice): string => `verification-warning-${choice.replace(/ /gu, "-")}`;

export function verificationWarningScreenMarkup(state: VerificationWarningScreenState): string {
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
