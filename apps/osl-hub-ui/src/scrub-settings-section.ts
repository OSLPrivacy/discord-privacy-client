import "./scrub-settings-section.css";

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

function scopeSummary(shortLabel: string, title: string, detail: string): string {
  return `<span class="scrub-scope-short machine-fact" aria-hidden="true">${shortLabel}</span><span class="sr-only"><strong>${title}</strong><small>${detail}</small></span>`;
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

  return `<section class="scrub-settings-section" aria-labelledby="scrub-settings-title">
    <header class="scrub-settings-heading">
      <h2 class="sr-only machine-fact" id="scrub-settings-title">Scrub</h2>
      <p><strong>Your messages never leave this device.</strong> Every scan and review stays local.</p>
    </header>
    <div class="scrub-scope-label machine-fact">Scrub scopes</div>
    <div class="scrub-scope-rows" aria-label="Scrub scopes">
      <details class="scrub-scope-row scrub-export-scope">
        <summary>${scopeSummary("Export", "Review a local message export", "Choose a file only after you acknowledge the account-risk warning.")}</summary>
        <div class="scrub-scope-body">${gatedRoute}${scanControls}</div>
      </details>
      <details class="scrub-scope-row scrub-signal-scope">
        <summary>${scopeSummary("Signals", "Change what OSL looks for", "Choose which kinds of messages are included in this local review.")}</summary>
        <div class="scrub-scope-body">${state.scrubCategoryChooserMarkup}${state.privacyScanResultsMarkup}</div>
      </details>
      <div class="scrub-scope-row scrub-autoscrub-scope">${state.autoScrubAssistantMarkup}</div>
      <details class="scrub-scope-row scrub-safety">
        <summary>${scopeSummary("Safety", "Before deleting anything", "Review the account, permanence, and verification limits.")}</summary>
        <div class="scrub-scope-body"><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, services, exports, and backups may retain copies. Only a service recheck can verify removal within its stated coverage.</p><p>Automatic deletion is unavailable in this build until the native one-shot reviewed-consent capability is available. Connect IMAP for read-only verification.</p><p>This build only gives manual directions. It does not delete app messages. Check the original app and delete each message yourself.</p></div>
      </details>
      <details class="scrub-scope-row privacy-technical">
        <summary>${scopeSummary("Technical", "Privacy and technical details", "Local expiry, app access, and capture-resistance facts.")}</summary>
        <div class="scrub-scope-body"><div class="setting-line"><span class="machine-fact">Default key expiry</span><strong class="machine-fact">${state.timer}</strong></div><div class="setting-line"><span class="machine-fact">Remote app access</span><strong class="machine-fact">Blocked</strong></div><div class="setting-line"><span><strong class="machine-fact">Windows capture resistance</strong><small>Always applied to OSL’s own window. Cameras, malware, and modified recipients can still capture content.</small></span><strong class="machine-fact">${state.screenshotProtectionEnabled ? "Active" : "Unavailable"}</strong></div></div>
      </details>
    </div>
  </section>`;
}
