import "./onboarding-tor.css";
import { choiceRadio, continueButton } from "./onboarding-controls";
import { firstRunTorScreenMarkup, type TorBootStatus } from "./tor-boot-orchestrator";

/** The network route must be chosen explicitly during onboarding. */
export type TorChoice = "tor" | "direct" | null;

export interface TorOnboardingState {
  choice: TorChoice;
  /** Null while choosing a route; populated exclusively from the sidecar
   * orchestrator's onStatus callback once a Tor attempt starts. */
  bootstrapStatus: TorBootStatus | null;
}

// TASK 5019: Direct remains the shipped default until the packaged-build 4900
// default-answer send proof is re-run green over the owned tunnel. The latest
// adjudicated proof is red, so selecting Tor here would be an unapproved flip.
export const SHIPPED_CONNECTION_DEFAULT: Exclude<TorChoice, null> = "direct";
export const initialTorOnboardingState = (): TorOnboardingState => ({ choice: SHIPPED_CONNECTION_DEFAULT, bootstrapStatus: null });

// Direct cancels any Tor attempt. Re-selecting Tor preserves the status stream
// projection so a radio re-render cannot rewind visible progress.
export function chooseTorRoute(state: TorOnboardingState, choice: Exclude<TorChoice, null>): TorOnboardingState {
  return { choice, bootstrapStatus: choice === "tor" ? state.bootstrapStatus : null };
}

/** Feed one status projection from startTorBootOrchestrator into the actual
 * first-run Tor route. There is no independent spinner or percentage clock. */
export function applyTorBootstrapStatus(state: TorOnboardingState, bootstrapStatus: TorBootStatus): TorOnboardingState {
  return { choice: "tor", bootstrapStatus };
}

/** Guards the Continue handler. A null choice can still arrive from a restored session. */
export function canContinuePastTorChoice(state: TorOnboardingState): boolean {
  return state.choice !== null;
}

/**
 * The two diagrams ARE the explanation. The old screen carried a sentence on
 * each card -- "may take longer", "your network provider can see" -- which
 * describes the cost in words a first-time user has no way to weigh. A dot that
 * takes six seconds to pick its way through three relays, next to one that
 * crosses in under two, says the same thing without asking them to imagine it.
 */
const PC_AND_SERVER = `
  <rect class="tor-ico" x="24" y="52" width="20" height="14" rx="2"/>
  <path class="tor-ico" d="M30 70 h8"/>
  <rect class="tor-ico" x="276" y="50" width="20" height="7" rx="1.5"/>
  <rect class="tor-ico" x="276" y="61" width="20" height="7" rx="1.5"/>`;

// Both icons are centred on the SAME line, y=59: the monitor's middle, and the
// gap between the server's two slabs. That is what lets the direct route be
// dead level instead of drifting 2px downhill across the card, which is how it
// read before the server was nudged down.
const TOR_ROUTE = "M50 59 L103 32 L160 88 L217 32 L270 59";
const DIRECT_ROUTE = "M50 59 L270 59";

function torDiagram(): string {
  return `<svg class="tor-diagram" viewBox="0 0 320 120" fill="none" aria-hidden="true">
    <path class="tor-route" d="${TOR_ROUTE}"/>
    ${PC_AND_SERVER}
    <circle class="tor-relay" cx="103" cy="32" r="4.5"/>
    <circle class="tor-relay" cx="160" cy="88" r="4.5"/>
    <circle class="tor-relay" cx="217" cy="32" r="4.5"/>
    <!-- r matches the relay ring's INNER radius (4.5 - 1.5/2 = 3.75), plus a
         hair so antialiasing leaves no ring of background showing when the dot
         parks inside a node. -->
    <circle class="tor-packet tor-packet-slow" r="3.9"/>
  </svg>`;
}

function directDiagram(): string {
  return `<svg class="tor-diagram" viewBox="0 0 320 120" fill="none" aria-hidden="true">
    <path class="tor-route" d="${DIRECT_ROUTE}"/>
    ${PC_AND_SERVER}
    <circle class="tor-packet tor-packet-fast" r="3.9"/>
  </svg>`;
}

/**
 * Render equally weighted choices. Neither is labelled recommended; the speed
 * contrast between the two animations is the only claim the screen makes.
 */
export function onboardingTorMarkup(state: TorOnboardingState): string {
  if (state.choice === "tor" && state.bootstrapStatus !== null) {
    return firstRunTorScreenMarkup(state.bootstrapStatus);
  }
  const card = (choice: Exclude<TorChoice, null>, title: string, diagram: string, caption: string): string => {
    const selected = state.choice === choice;
    return `<label class="tor-choice-card${selected ? " selected" : ""}">
      <input class="sr-only" type="radio" name="tor-route" value="${choice}" aria-label="${title}"${selected ? " checked" : ""}/>
      <span class="tor-card-head">${choiceRadio()}<strong>${title}</strong></span>
      ${diagram}
      <span class="tor-card-caption">${caption}</span>
    </label>`;
  };

  return `<section class="tor-onboarding" aria-labelledby="tor-onboarding-heading">
    <h1 id="tor-onboarding-heading" tabindex="-1" class="tor-title">Connection choice</h1>
    <fieldset class="tor-choice-grid"><legend class="sr-only">Connection route</legend>
      ${card("tor", "Tor", torDiagram(), "travel time · 2–6 s")}
      ${card("direct", "Direct", directDiagram(), "travel time · under 1 s")}
    </fieldset>
    <div class="tor-mullvad-status" aria-label="Mullvad status"><strong>Mullvad</strong><span>You can use both. Neither replaces the other.</span></div>
    <div class="setup-footer onboarding-actions">${continueButton("data-tor-choice-continue", "tor-continue")}</div>
  </section>`;
}
