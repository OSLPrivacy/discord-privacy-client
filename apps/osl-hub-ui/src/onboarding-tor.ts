import "./onboarding-tor.css";

/** The network route must be chosen explicitly during onboarding. */
export type TorChoice = "tor" | "direct" | null;

export interface TorOnboardingState {
  choice: TorChoice;
}

export const initialTorOnboardingState = (): TorOnboardingState => ({ choice: null });

export function chooseTorRoute(_state: TorOnboardingState, choice: Exclude<TorChoice, null>): TorOnboardingState {
  return { choice };
}

/** There is intentionally no default: the next step stays unavailable until a choice is made. */
export function canContinuePastTorChoice(state: TorOnboardingState): boolean {
  return state.choice !== null;
}

/**
 * Render equally weighted choices. Each names its own cost, so neither is
 * presented as the product's recommended or inherently safer route.
 */
export function onboardingTorMarkup(state: TorOnboardingState): string {
  const card = (choice: Exclude<TorChoice, null>, title: string, downside: string, icon: string): string => {
    const selected = state.choice === choice;
    return `<label class="tor-choice-card${selected ? " selected" : ""}">
      <input type="radio" name="tor-route" value="${choice}"${selected ? " checked" : ""}/>
      <span class="tor-choice-icon tor-choice-icon-${icon}" aria-hidden="true"></span>
      <span class="tor-choice-copy"><strong>${title}</strong><small>${downside}</small></span>
    </label>`;
  };

  return `<section class="tor-onboarding" aria-labelledby="tor-onboarding-heading">
    <p class="eyebrow">Connection choice</p>
    <h1 id="tor-onboarding-heading" tabindex="-1">Choose how OSL connects</h1>
    <p class="compact-lead">Pick one before continuing. You can review this choice later in Settings.</p>
    <fieldset class="tor-choice-grid"><legend class="sr-only">Connection route</legend>
      ${card("tor", "Use Tor", "Connections may take longer and may not work on every network.", "tor")}
      ${card("direct", "Connect directly", "Your network provider can see that this device connects to OSL’s server.", "direct")}
    </fieldset>
    <p class="tor-choice-note" role="status">${state.choice === null ? "Choose a connection route to continue." : "Your choice will be saved before OSL sends or fetches anything."}</p>
    <button class="button primary" data-tor-choice-continue type="button" ${canContinuePastTorChoice(state) ? "" : "disabled"}>Continue</button>
  </section>`;
}
