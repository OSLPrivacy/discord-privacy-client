import "./scrub-consent-gate.css";
import { executeScrubDryRun, type ScrubDryRunRequest } from "./scrub-engine-host";
import { renderScrubRoute, type ScrubRouteState, type ScrubRouteStep } from "./scrub-route";

export interface ScrubConsentGateRequest {
  /** Stable adapter identifier; native enforcement must authorize this exact service. */
  serviceId: string;
  /** Human-facing service name used in the acknowledgement the owner must type. */
  serviceName: string;
  /** The K1-reviewed, service-specific account-termination warning. */
  warning: string;
}

export interface ScrubConsentGateState {
  checked: boolean;
  typedAcknowledgement: string;
}

export type ScrubConsentGateResult =
  | { allowed: true; serviceId: string }
  | { allowed: false; reason: "consent-checkbox-required" | "typed-acknowledgement-required" | "invalid-service" };

/**
 * These values mirror scrub-consent-gate.css. The warning must never be less
 * prominent than the control that advances to deletion.
 */
export const SCRUB_CONSENT_GATE_PRESENTATION = {
  warningFontSizePx: 14,
  warningFontWeight: 600,
  proceedFontSizePx: 14,
  proceedFontWeight: 600,
} as const;

export function defaultScrubConsentGateState(): ScrubConsentGateState {
  return { checked: false, typedAcknowledgement: "" };
}

export function consentAcknowledgementForService(serviceName: string): string {
  return `I understand that Scrub can permanently terminate my ${serviceName} account.`;
}

export function evaluateScrubConsentGate(
  request: ScrubConsentGateRequest,
  state: ScrubConsentGateState,
): ScrubConsentGateResult {
  if (!validService(request)) return { allowed: false, reason: "invalid-service" };
  if (!state.checked) return { allowed: false, reason: "consent-checkbox-required" };
  if (state.typedAcknowledgement !== consentAcknowledgementForService(request.serviceName)) {
    return { allowed: false, reason: "typed-acknowledgement-required" };
  }
  return { allowed: true, serviceId: request.serviceId };
}

/**
 * This is deliberately rendered with the destructive action, not in Settings.
 * The caller must re-evaluate immediately before requesting native execution;
 * this UI result never authorizes a deletion on its own.
 */
export function scrubConsentGateMarkup(
  request: ScrubConsentGateRequest,
  state: ScrubConsentGateState,
): string {
  const result = evaluateScrubConsentGate(request, state);
  const serviceName = escapeHtml(request.serviceName);
  const warning = escapeHtml(request.warning);
  const acknowledgement = consentAcknowledgementForService(request.serviceName);
  const disabled = result.allowed ? "" : ' disabled aria-disabled="true"';

  return `<section class="scrub-consent-gate" data-scrub-action-path="delete" aria-labelledby="scrub-consent-heading">
    <h2 id="scrub-consent-heading">Before deleting from ${serviceName}</h2>
    <p class="scrub-consent-warning" role="alert">${warning}</p>
    <label class="scrub-consent-check"><input type="checkbox" name="scrub-consent-${escapeHtml(request.serviceId)}" value="accepted"> I understand this risk applies to my ${serviceName} account.</label>
    <label class="scrub-consent-typed" for="scrub-consent-acknowledgement">Type this exactly to continue: <strong>${escapeHtml(acknowledgement)}</strong><input id="scrub-consent-acknowledgement" name="scrub-consent-acknowledgement" type="text" autocomplete="off" spellcheck="false" value="${escapeHtml(state.typedAcknowledgement)}"></label>
    <button class="button danger scrub-consent-proceed" type="button"${disabled}>Continue to delete from ${serviceName}</button>
  </section>`;
}

/**
 * The shipping Scrub route is deliberately owned by the consent gate.  Keeping
 * the route import here means main has no ungated route or engine entry point.
 */
export function scrubConsentGatedRouteMarkup(
  request: ScrubConsentGateRequest,
  state: ScrubConsentGateState,
  routeState: ScrubRouteState,
  requestedStep: ScrubRouteStep,
  opened: boolean,
): string {
  const consent = evaluateScrubConsentGate(request, state);
  const route = consent.allowed && opened
    ? renderScrubRoute(routeState, requestedStep)
    : "";
  return `${scrubConsentGateMarkup(request, state)}${route}`;
}

/**
 * The only shipped deletion-engine entry point.  The engine host permanently
 * selects dryRun, and this wrapper requires fresh service-bound consent before
 * even a preview can be requested.
 */
export async function executeConsentedScrubDryRun(
  request: ScrubConsentGateRequest,
  state: ScrubConsentGateState,
  dryRunRequest: ScrubDryRunRequest,
): ReturnType<typeof executeScrubDryRun> {
  if (!evaluateScrubConsentGate(request, state).allowed) {
    throw new Error("Scrub consent is required before scanning or deletion preview");
  }
  return executeScrubDryRun(dryRunRequest);
}

function validService(request: ScrubConsentGateRequest): boolean {
  return request.serviceId.trim().length > 0
    && request.serviceId.length <= 80
    && request.serviceName.trim().length > 0
    && request.serviceName.length <= 80
    && request.warning.trim().length > 0
    && request.warning.length <= 4_096;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}
