import "./messaging-risk-page.css";
import { continueButton } from "./onboarding-controls";
import { choiceTick } from "./scrub-account-choice";

/**
 * TASK 3112 -- the risk page shown before a messaging service is connected.
 *
 * The five facts are the same five `services::MESSAGING_RISK_FACTS` (TASK
 * 3109) writes into every service's agreement record, copied here rather than
 * fetched so the page never has to wait on a round trip to show them. "Read
 * service terms" calls the real `get_service_terms_address` command (TASK
 * 1406) for the service on screen. The tick is the only thing that unlocks
 * Continue: nothing here starts ticked, and unticking locks it again.
 */

export const MESSAGING_RISK_FACTS: readonly string[] = [
  "OSL controls the app",
  "this may break that service's rules",
  "the account may be suspended",
  "OSL cannot remove that risk",
  "you can turn it off",
];

/** The step this screen owns, and the named steps either side of it. */
export const MESSAGING_RISK_STEP = "risk";
export const MESSAGING_RISK_BACK_STEP = "account";
export const MESSAGING_RISK_NEXT_STEP = "connect";

export const GET_SERVICE_TERMS_ADDRESS_COMMAND = "get_service_terms_address";

export interface MessagingRiskState {
  readonly agreed: boolean;
}

/** Nothing is agreed for the owner: the risk is accepted only by ticking it. */
export function initialMessagingRiskState(): MessagingRiskState {
  return { agreed: false };
}

export function toggleMessagingRiskAgreement(state: MessagingRiskState): MessagingRiskState {
  return { agreed: !state.agreed };
}

export function canContinueFromMessagingRisk(state: MessagingRiskState): boolean {
  return state.agreed;
}

/** Back leaves the tick alone and writes nothing. */
export function backFromMessagingRisk(): typeof MESSAGING_RISK_BACK_STEP {
  return MESSAGING_RISK_BACK_STEP;
}

export type MessagingRiskContinueResult =
  | { readonly outcome: "advanced"; readonly step: typeof MESSAGING_RISK_NEXT_STEP }
  | { readonly outcome: "refused"; readonly step: typeof MESSAGING_RISK_STEP; readonly reason: "risk-not-agreed" };

/** The only way off this screen forward: ticked opens the next step, every time. */
export function continueFromMessagingRisk(state: MessagingRiskState): MessagingRiskContinueResult {
  if (!canContinueFromMessagingRisk(state)) {
    return { outcome: "refused", step: MESSAGING_RISK_STEP, reason: "risk-not-agreed" };
  }
  return { outcome: "advanced", step: MESSAGING_RISK_NEXT_STEP };
}

export interface ServiceTermsAddress {
  readonly serviceId: string;
  readonly termsAddress: string;
}

export type MessagingRiskInvoke = (
  command: typeof GET_SERVICE_TERMS_ADDRESS_COMMAND,
  payload: { serviceId: string },
) => Promise<ServiceTermsAddress>;

/** Reads the real terms address for the service on screen; opens nothing itself. */
export async function readServiceTerms(
  serviceId: string,
  invoke: MessagingRiskInvoke,
): Promise<ServiceTermsAddress> {
  return invoke(GET_SERVICE_TERMS_ADDRESS_COMMAND, { serviceId });
}

function factRow(fact: string): string {
  return `<li class="mr-fact">${escapeHtml(fact)}</li>`;
}

export function messagingRiskPageMarkup(serviceId: string, serviceName: string, state: MessagingRiskState): string {
  const ready = canContinueFromMessagingRisk(state);
  return `<section class="messaging-risk-screen" data-messaging-service="${escapeHtml(serviceId)}" data-messaging-step="${MESSAGING_RISK_STEP}" data-agreed="${state.agreed ? "yes" : "no"}">
  <header class="mr-head">
    <h1 id="route-heading" class="mr-title">Messaging risk</h1>
    <p class="mr-lead">Before OSL connects to ${escapeHtml(serviceName)}, here is what that means.</p>
  </header>
  <ul class="mr-facts" aria-label="Messaging risk facts">
    ${MESSAGING_RISK_FACTS.map(factRow).join("\n    ")}
  </ul>
  <button class="button ghost mr-terms" id="messaging-risk-terms" data-messaging-risk-terms="${escapeHtml(serviceId)}" type="button">Read service terms</button>
  <label class="mr-agree"><input class="sr-only mr-agree-tick" id="messaging-risk-agree" type="checkbox" name="messaging-risk-agree"${state.agreed ? " checked" : ""}/>${choiceTick()}<span>I understand this risk.</span></label>
  <div class="mr-footer setup-footer onboarding-actions">
    <button class="button ghost mr-back" id="messaging-risk-back" data-messaging-back="${MESSAGING_RISK_BACK_STEP}" type="button">Back</button>
    ${continueButton(`id="messaging-risk-continue" data-messaging-continue="${MESSAGING_RISK_NEXT_STEP}"${ready ? "" : ' disabled aria-disabled="true"'}`, "mr-continue")}
  </div>
</section>`;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
