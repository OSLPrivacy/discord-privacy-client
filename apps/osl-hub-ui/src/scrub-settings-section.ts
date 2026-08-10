import {
  evaluateScrubConsentGate,
  scrubConsentGatedRouteMarkup,
  type ScrubConsentGateRequest,
  type ScrubConsentGateState,
} from "./scrub-consent-gate";
import type { ScrubRouteState, ScrubRouteStep } from "./scrub-route";
import type { ScrubSignalGroup } from "./scrub";

export interface ScrubSettingsSectionState {
  proActive: boolean;
  scanBusy: boolean;
  findingCount: number | null;
  consentRequest: ScrubConsentGateRequest;
  consentState: ScrubConsentGateState;
  routeStep: ScrubRouteStep;
  routeOpened: boolean;
  routeAccountSelected: boolean;
  routeCategories: readonly ScrubSignalGroup[];
  timer: string;
  screenshotProtectionEnabled: boolean;
  scrubCategoryChooserMarkup: string;
  privacyScanResultsMarkup: string;
  autoScrubAssistantMarkup: string;
}

export function privacySettingsContent(state: ScrubSettingsSectionState): string {
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${state.scanBusy ? "disabled" : ""}" for="privacy-export-input">${state.scanBusy ? "Scanning…" : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${state.scanBusy ? "disabled" : ""}/>${state.findingCount !== null ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  const routeState: ScrubRouteState = {
    accounts: [{ id: "local-export", label: "Local message export", detail: "TXT, CSV, or JSON on this device" }],
    selectedAccountIds: state.routeAccountSelected ? ["local-export"] : [],
    selectedCategories: [...state.routeCategories],
    scan: { state: state.scanBusy ? "scanning" : state.findingCount !== null ? "complete" : "not-started", findings: state.findingCount ?? 0 },
  };
  const consent = evaluateScrubConsentGate(state.consentRequest, state.consentState);
  const gatedRoute = scrubConsentGatedRouteMarkup(
    state.consentRequest,
    state.consentState,
    routeState,
    state.routeStep,
    state.routeOpened,
  );
  const scanControls = consent.allowed && state.routeOpened && state.routeStep === "scan" ? scanActions : "";
  return `<h2 class="machine-fact">Scrub</h2><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Every scan and review stays local.</p>${gatedRoute}${scanControls}${state.scrubCategoryChooserMarkup}${state.privacyScanResultsMarkup}${state.autoScrubAssistantMarkup}<details class="safety-disclosure scrub-safety"><summary class="machine-fact">Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, services, exports, and backups may retain copies. Only a service recheck can verify removal within its stated coverage.</p><p>Automatic deletion is unavailable in this build until the native one-shot reviewed-consent capability is available. Connect IMAP for read-only verification.</p><p>This build only gives manual directions. It does not delete app messages. Check the original app and delete each message yourself.</p></div></details><details class="privacy-technical settings-disclosure"><summary class="machine-fact">Privacy and technical details</summary><div class="setting-line"><span class="machine-fact">Default key expiry</span><strong class="machine-fact">${state.timer}</strong></div><div class="setting-line"><span class="machine-fact">Remote app access</span><strong class="machine-fact">Blocked</strong></div><div class="setting-line"><span><strong class="machine-fact">Windows capture resistance</strong><small>Always applied to OSL’s own window. Cameras, malware, and modified recipients can still capture content.</small></span><strong class="machine-fact">${state.screenshotProtectionEnabled ? "Active" : "Unavailable"}</strong></div></details>`;
}
