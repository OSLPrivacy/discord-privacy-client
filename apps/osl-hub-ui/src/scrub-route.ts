import { scrubSignalDefinitions, type ScrubSignalGroup } from "./scrub";

export type ScrubRouteStep = "choose" | "scan" | "review";

export interface ScrubRouteAccount {
  id: string;
  label: string;
  detail: string;
}

export interface ScrubRouteScan {
  state: "not-started" | "scanning" | "complete";
  findings: number;
}

export interface ScrubRouteState {
  accounts: readonly ScrubRouteAccount[];
  selectedAccountIds: readonly string[];
  selectedCategories: readonly ScrubSignalGroup[];
  scan: ScrubRouteScan;
}

const stepOrder: readonly ScrubRouteStep[] = ["choose", "scan", "review"];

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function hasScrubScope(state: ScrubRouteState): boolean {
  const accountIds = new Set(state.accounts.map(({ id }) => id));
  const categories = new Set(scrubSignalDefinitions.map(({ id }) => id));
  return state.selectedAccountIds.some((id) => accountIds.has(id))
    && state.selectedCategories.some((category) => categories.has(category));
}

export function furthestScrubRouteStep(state: ScrubRouteState): ScrubRouteStep {
  if (!hasScrubScope(state)) return "choose";
  if (state.scan.state !== "complete") return "scan";
  return "review";
}

/** A requested screen is never allowed to jump ahead of the completed route state. */
export function scrubRouteStep(state: ScrubRouteState, requested: ScrubRouteStep): ScrubRouteStep {
  return stepOrder.indexOf(requested) <= stepOrder.indexOf(furthestScrubRouteStep(state))
    ? requested
    : furthestScrubRouteStep(state);
}

export function renderScrubRoute(state: ScrubRouteState, requested: ScrubRouteStep = "choose"): string {
  const step = scrubRouteStep(state, requested);
  const scopeReady = hasScrubScope(state);
  const completed = state.scan.state === "complete";
  const progress = stepOrder.map((name, index) => {
    const active = name === step;
    const done = index < stepOrder.indexOf(step);
    return `<li class="scrub-route-progress-step${active ? " active" : ""}${done ? " complete" : ""}"${active ? ' aria-current="step"' : ""}>${index + 1}. ${name === "choose" ? "Choose" : name === "scan" ? "Scan" : "Review"}</li>`;
  }).join("");
  let body: string;

  if (step === "choose") {
    const accounts = state.accounts.map((account) => `<label class="scrub-route-choice"><input type="checkbox" name="scrub-account" value="${escapeHtml(account.id)}"${state.selectedAccountIds.includes(account.id) ? " checked" : ""}><span><strong>${escapeHtml(account.label)}</strong><small>${escapeHtml(account.detail)}</small></span></label>`).join("");
    const categories = scrubSignalDefinitions.map((category) => `<label class="scrub-route-choice"><input type="checkbox" name="scrub-category" value="${category.id}"${state.selectedCategories.includes(category.id) ? " checked" : ""}><span><strong>${escapeHtml(category.label)}</strong><small>${escapeHtml(category.detail)}</small></span></label>`).join("");
    body = `<section class="scrub-route-panel" aria-labelledby="scrub-route-choose-title"><h2 id="scrub-route-choose-title">Choose what to scan</h2><p>Choose one or more accounts and categories. The scan stays on this device.</p><fieldset><legend>Accounts</legend><div class="scrub-route-choice-list">${accounts || "<p>No accounts are available to scan.</p>"}</div></fieldset><fieldset><legend>Categories</legend><div class="scrub-route-choice-list">${categories}</div></fieldset><button class="button primary" type="button" data-scrub-route-next="scan"${scopeReady ? "" : " disabled"}>Continue to scan</button></section>`;
  } else if (step === "scan") {
    const scanCopy = state.scan.state === "scanning"
      ? "Scanning the selected accounts on this device."
      : "Start a local scan. Nothing is deleted or sent to a service.";
    body = `<section class="scrub-route-panel" aria-labelledby="scrub-route-scan-title"><h2 id="scrub-route-scan-title">Scan selected content</h2><p>${scanCopy}</p><p class="scrub-route-status" aria-live="polite">${completed ? `${state.scan.findings} items are ready for review.` : state.scan.state === "scanning" ? "Scan in progress." : "Scan has not started."}</p><button class="button primary" type="button" data-scrub-route-scan${state.scan.state === "scanning" ? " disabled" : ""}>${completed ? "Scan again" : "Start scan"}</button><button class="button" type="button" data-scrub-route-next="review"${completed ? "" : " disabled"}>Review findings</button></section>`;
  } else {
    body = `<section class="scrub-route-panel" aria-labelledby="scrub-route-review-title"><h2 id="scrub-route-review-title">Review scan results</h2><p>${state.scan.findings} ${state.scan.findings === 1 ? "item is" : "items are"} ready for your review. Nothing is deleted from this route.</p><button class="button" type="button" data-scrub-route-back="scan">Back to scan</button></section>`;
  }

  return `<section class="scrub-route" data-scrub-route-step="${step}" aria-label="Scrub route"><ol class="scrub-route-progress">${progress}</ol>${body}</section>`;
}
