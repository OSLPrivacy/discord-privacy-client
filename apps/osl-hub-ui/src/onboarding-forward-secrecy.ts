import "./onboarding-tor.css";

/** The delivery/recovery tradeoff must be selected rather than inferred. */
export type ForwardSecrecyChoice = "protect-past" | "keep-group-delivery" | null;

export interface ForwardSecrecyOnboardingState {
  choice: ForwardSecrecyChoice;
}

export const initialForwardSecrecyOnboardingState = (): ForwardSecrecyOnboardingState => ({ choice: null });

export function chooseForwardSecrecyMode(
  _state: ForwardSecrecyOnboardingState,
  choice: Exclude<ForwardSecrecyChoice, null>,
): ForwardSecrecyOnboardingState {
  return { choice };
}

export function canContinuePastForwardSecrecyChoice(state: ForwardSecrecyOnboardingState): boolean {
  return state.choice !== null;
}

export function onboardingForwardSecrecyMarkup(state: ForwardSecrecyOnboardingState): string {
  const card = (choice: Exclude<ForwardSecrecyChoice, null>, title: string, downside: string, icon: string): string => {
    const selected = state.choice === choice;
    return `<label class="tor-choice-card${selected ? " selected" : ""}">
      <input type="radio" name="forward-secrecy-mode" value="${choice}"${selected ? " checked" : ""}/>
      <span class="tor-choice-icon tor-choice-icon-${icon}" aria-hidden="true"></span>
      <span class="tor-choice-copy"><strong>${title}</strong><small>${downside}</small></span>
    </label>`;
  };
  return `<section class="tor-onboarding" aria-labelledby="forward-secrecy-heading">
    <p class="eyebrow">Message recovery choice</p>
    <h1 id="forward-secrecy-heading" tabindex="-1">Choose message protection</h1>
    <p class="compact-lead">Pick one before continuing. You can review this choice later in Settings.</p>
    <fieldset class="tor-choice-grid"><legend class="sr-only">Message protection</legend>
      ${card("protect-past", "Protect past messages", "A stolen data copy cannot reconstruct earlier message keys. Cost: a restart begins a fresh chain and late messages are lost.", "tor")}
      ${card("keep-group-delivery", "Keep group delivery as today", "Cost: a persisted snapshot can recover prior message keys.", "direct")}
    </fieldset>
    <p class="tor-choice-note" role="status">${state.choice === null ? "Choose how message recovery works to continue." : "Your choice will be saved before OSL sends messages."}</p>
    <button class="button primary" data-forward-secrecy-continue type="button" ${canContinuePastForwardSecrecyChoice(state) ? "" : "disabled"}>Continue</button>
  </section>`;
}
