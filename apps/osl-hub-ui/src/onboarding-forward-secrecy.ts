import "./onboarding-forward-secrecy.css";
import { choiceRadio, continueButton } from "./onboarding-controls";

/** The delivery/recovery tradeoff must be selected rather than inferred. */
export type ForwardSecrecyChoice = "protect-past" | "keep-group-delivery";

export interface ForwardSecrecyOnboardingState {
  choice: ForwardSecrecyChoice | null;
}

const forwardSecrecyChoices: readonly ForwardSecrecyChoice[] = ["protect-past", "keep-group-delivery"];

export const initialForwardSecrecyOnboardingState = (): ForwardSecrecyOnboardingState => ({ choice: null });

export function chooseForwardSecrecyMode(
  state: ForwardSecrecyOnboardingState,
  choice: unknown,
): ForwardSecrecyOnboardingState {
  return forwardSecrecyChoices.includes(choice as ForwardSecrecyChoice)
    ? { choice: choice as ForwardSecrecyChoice }
    : state;
}

/** Guards the Continue handler against a restored session with no choice. */
export function canContinuePastForwardSecrecyChoice(state: ForwardSecrecyOnboardingState): boolean {
  return state.choice !== null && state.choice !== undefined;
}

/**
 * One lock, drawn twice per card in two states. `open` lifts the shackle out of
 * the body; the body is identical either way, so the only thing that changes
 * between the two cards is whether the shackle is down.
 */
function lock(): string {
  return `<svg class="fs-lock" viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path class="fs-lock-shackle" d="M8 11 V8 a4 4 0 0 1 8 0 v3"/>
    <rect x="5" y="11" width="14" height="9" rx="2"/>
  </svg>`;
}

/**
 * 2026-08-06 rewrite, then restyle. The original copy read "a stolen data copy
 * cannot reconstruct earlier message keys" -- cryptographer language on a screen
 * a first-time user meets in their first two minutes. Nobody chooses well from
 * that, so nobody chooses; they pick whichever word sounds safer.
 *
 * Same two choices, same two costs. What does the explaining now is the pair of
 * animations: three green locks that get rattled and hold, against three amber
 * locks whose shackles swing open.
 */
export function onboardingForwardSecrecyMarkup(state: ForwardSecrecyOnboardingState): string {
  const card = (
    choice: ForwardSecrecyChoice,
    title: string,
    tradeoff: string,
    lockClass: string,
    label: string,
  ): string => {
    const selected = state.choice === choice;
    return `<label class="fs-choice-card${selected ? " selected" : ""}">
      <input class="sr-only" type="radio" name="forward-secrecy-mode" value="${choice}"${selected ? " checked" : ""}/>
      <span class="fs-card-head">${choiceRadio()}<strong>${title}</strong></span>
      <span class="fs-locks ${lockClass}" role="img" aria-label="${label}">${lock()}${lock()}${lock()}</span>
      <small class="fs-tradeoff">${tradeoff}</small>
    </label>`;
  };

  return `<section class="fs-onboarding" aria-labelledby="forward-secrecy-heading">
    <h1 id="forward-secrecy-heading" tabindex="-1" class="fs-title">Forward secrecy</h1>
    <p class="fs-question">If someone gets into this computer,<br/>what happens to old messages?</p>
    <fieldset class="fs-choice-grid"><legend class="sr-only">Old messages</legend>
      ${card(
        "protect-past",
        "Old messages stay locked",
        "A break-in reads nothing. A badly timed restart can lose a message in transit.",
        "fs-locks-held",
        "three locks being rattled and staying shut",
      )}
      ${card(
        "keep-group-delivery",
        "Old messages stay readable to you",
        "Nothing is ever lost, even joining late. A copy of your data reads what you sent.",
        "fs-locks-open",
        "three locks swinging open",
      )}
    </fieldset>
    <div class="setup-footer onboarding-actions">${continueButton(`data-forward-secrecy-continue ${canContinuePastForwardSecrecyChoice(state) ? "" : 'disabled aria-disabled="true"'}`, "fs-continue")}</div>
  </section>`;
}
