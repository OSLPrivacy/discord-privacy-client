import { invoke } from "@tauri-apps/api/core";
import "./scrub-setup.css";

export type ScrubSetupFrequency = "daily" | "weekly" | "monthly";
export type ScrubSetupNotice = "before_scan" | "after_scan" | "quiet";

export interface ScrubSetupAccount {
  id: string;
  label: string;
  detail: string;
}

export interface ScrubSetupDraft {
  selectedScanAccounts: Set<string>;
  autoScrub: boolean;
  automaticSchedule: ScrubSetupFrequency;
  noticeSetting: ScrubSetupNotice;
}

export interface ScrubSetupCommand {
  selectedScanAccounts: string[];
  automaticSchedule: ScrubSetupFrequency | null;
  noticeSetting: ScrubSetupNotice | null;
  notNow: boolean;
}

export interface ScrubSetupSummary {
  accountCount: number;
  automaticSchedule: ScrubSetupFrequency | null;
  noticeSetting: ScrubSetupNotice | null;
}

export interface ScrubSetupCallbacks {
  onChange?: (draft: ScrubSetupDraft) => void;
  onScanNow: (selectedScanAccounts: string[]) => void;
  onContinue: (summary: ScrubSetupSummary, draft: ScrubSetupDraft) => void;
  onNotNow: (summary: ScrubSetupSummary) => void;
  onBack: () => void;
  onError?: (message: string) => void;
}

type ScrubSetupRoot = Pick<ParentNode, "querySelector" | "querySelectorAll">;

const frequencies: readonly ScrubSetupFrequency[] = ["daily", "weekly", "monthly"];
const notices: readonly ScrubSetupNotice[] = ["before_scan", "after_scan", "quiet"];

export function initialScrubSetupDraft(): ScrubSetupDraft {
  return {
    selectedScanAccounts: new Set<string>(),
    autoScrub: true,
    automaticSchedule: "weekly",
    noticeSetting: "before_scan",
  };
}

export function scrubSetupMarkup(
  accounts: readonly ScrubSetupAccount[],
  draft: ScrubSetupDraft,
  busy = false,
  status = "",
): string {
  const accountRows = accounts.map((account) => {
    const checked = draft.selectedScanAccounts.has(account.id) ? " checked" : "";
    return `<label class="scrub-setup-account"><input name="scrub-setup-account" type="checkbox" value="${escapeHtml(account.id)}"${checked}${busy ? " disabled" : ""}/><span><strong>${escapeHtml(account.label)}</strong><small>${escapeHtml(account.detail)}</small></span></label>`;
  }).join("");
  const hasAccounts = draft.selectedScanAccounts.size > 0;
  const continueDisabled = busy || !hasAccounts || !draft.autoScrub;
  return `<section class="scrub-setup" aria-labelledby="route-heading">
    <p class="eyebrow">Optional privacy check</p>
    <h1 id="route-heading" tabindex="-1">Scrub and AutoScrub</h1>
    <p class="compact-lead">Choose the signed-in accounts OSL may scan. You can scan them now or save a recurring local review.</p>
    <fieldset class="scrub-setup-accounts"><legend>Accounts</legend><div>${accountRows || `<p class="scrub-setup-empty">No signed-in accounts are available.</p>`}</div></fieldset>
    <div class="scrub-setup-scan-row"><button class="button" id="scan-now-scrub-setup" type="button"${busy || !hasAccounts ? " disabled" : ""}>Scan now</button><small>Starts an attended scan for the checked accounts. Nothing is deleted.</small></div>
    <label class="scrub-setup-autoscrub"><input id="enable-autoscrub" type="checkbox"${draft.autoScrub ? " checked" : ""}${busy ? " disabled" : ""}/><span><strong>AutoScrub</strong><small>Save a recurring scan plan for the checked accounts.</small></span></label>
    <fieldset class="scrub-setup-options" data-autoscrub-options${draft.autoScrub ? "" : ` disabled aria-disabled="true"`}><legend>Frequency</legend><div class="scrub-setup-radio-row">${frequencyChoice("daily", "Daily", draft, busy)}${frequencyChoice("weekly", "Weekly", draft, busy)}${frequencyChoice("monthly", "Monthly", draft, busy)}</div></fieldset>
    <fieldset class="scrub-setup-options"><legend>Notices</legend><div class="scrub-setup-radio-row">${noticeChoice("before_scan", "Before each scan", draft, busy)}${noticeChoice("after_scan", "After each scan", draft, busy)}${noticeChoice("quiet", "Quiet", draft, busy)}</div></fieldset>
    <p class="scrub-setup-status" role="status" aria-live="polite">${escapeHtml(status)}</p>
    <div class="setup-footer onboarding-actions scrub-setup-actions"><button class="button ghost" id="back-scrub-setup" type="button"${busy ? " disabled" : ""}>Back</button><button class="button primary" id="continue-scrub-setup" type="button"${continueDisabled ? " disabled" : ""}>${busy ? "Saving…" : "Continue"}</button><button class="text-button" id="not-now-scrub-setup" type="button"${busy ? " disabled" : ""}>Not now</button></div>
  </section>`;
}

export function scrubSetupCommand(draft: ScrubSetupDraft): ScrubSetupCommand {
  return {
    selectedScanAccounts: [...draft.selectedScanAccounts],
    automaticSchedule: draft.autoScrub ? draft.automaticSchedule : null,
    noticeSetting: draft.noticeSetting,
    notNow: false,
  };
}

export function scrubSetupNotNowCommand(): ScrubSetupCommand {
  return {
    selectedScanAccounts: [],
    automaticSchedule: null,
    noticeSetting: null,
    notNow: true,
  };
}

export async function saveScrubSetup(command: ScrubSetupCommand): Promise<ScrubSetupSummary> {
  const raw = await invoke<unknown>("save_scrub_setup", { command });
  return parseScrubSetupSummary(raw, command);
}

/**
 * Connects the rendered setup controls to the native store. The caller owns
 * rerendering; this binder keeps the live draft authoritative between paints.
 */
export function bindScrubSetupControls(
  root: ScrubSetupRoot,
  draft: ScrubSetupDraft,
  callbacks: ScrubSetupCallbacks,
): void {
  let busy = false;
  const accountInputs = [...root.querySelectorAll<HTMLInputElement>('input[name="scrub-setup-account"]')];
  const autoScrub = root.querySelector<HTMLInputElement>("#enable-autoscrub");
  const frequencyInputs = [...root.querySelectorAll<HTMLInputElement>('input[name="scrub-setup-frequency"]')];
  const noticeInputs = [...root.querySelectorAll<HTMLInputElement>('input[name="scrub-setup-notice"]')];
  const scanNow = root.querySelector<HTMLButtonElement>("#scan-now-scrub-setup");
  const continueButton = root.querySelector<HTMLButtonElement>("#continue-scrub-setup");
  const notNowButton = root.querySelector<HTMLButtonElement>("#not-now-scrub-setup");
  const backButton = root.querySelector<HTMLButtonElement>("#back-scrub-setup");

  const selectedAccounts = (): string[] => accountInputs.filter((input) => input.checked).map((input) => input.value);
  const notifyChange = (): void => callbacks.onChange?.(copyDraft(draft));
  const syncControls = (): void => {
    const hasAccounts = draft.selectedScanAccounts.size > 0;
    for (const input of accountInputs) input.disabled = busy;
    if (autoScrub) autoScrub.disabled = busy;
    if (scanNow) scanNow.disabled = busy || !hasAccounts;
    if (continueButton) continueButton.disabled = busy || !hasAccounts || !draft.autoScrub;
    if (notNowButton) notNowButton.disabled = busy;
    if (backButton) backButton.disabled = busy;
    for (const input of frequencyInputs) input.disabled = busy || !draft.autoScrub;
    for (const input of noticeInputs) input.disabled = busy;
  };
  const reportError = (failure: unknown): void => {
    const message = failure instanceof Error ? failure.message : "Scrub setup could not be saved";
    callbacks.onError?.(message);
  };

  for (const input of accountInputs) input.addEventListener("change", () => {
    draft.selectedScanAccounts = new Set(selectedAccounts());
    syncControls();
    notifyChange();
  });
  autoScrub?.addEventListener("change", () => {
    draft.autoScrub = autoScrub.checked;
    syncControls();
    notifyChange();
  });
  for (const input of frequencyInputs) input.addEventListener("change", () => {
    if (!input.checked || !isFrequency(input.value)) return;
    draft.automaticSchedule = input.value;
    notifyChange();
  });
  for (const input of noticeInputs) input.addEventListener("change", () => {
    if (!input.checked || !isNotice(input.value)) return;
    draft.noticeSetting = input.value;
    notifyChange();
  });
  scanNow?.addEventListener("click", () => {
    if (busy || draft.selectedScanAccounts.size === 0) return;
    callbacks.onScanNow([...draft.selectedScanAccounts]);
  });
  continueButton?.addEventListener("click", async () => {
    if (busy || draft.selectedScanAccounts.size === 0 || !draft.autoScrub) return;
    const submittedDraft = copyDraft(draft);
    const command = scrubSetupCommand(submittedDraft);
    busy = true;
    syncControls();
    try {
      const summary = await saveScrubSetup(command);
      callbacks.onContinue(summary, submittedDraft);
    } catch (failure) {
      reportError(failure);
    } finally {
      busy = false;
      syncControls();
    }
  });
  notNowButton?.addEventListener("click", async () => {
    if (busy) return;
    busy = true;
    syncControls();
    try {
      callbacks.onNotNow(await saveScrubSetup(scrubSetupNotNowCommand()));
    } catch (failure) {
      reportError(failure);
    } finally {
      busy = false;
      syncControls();
    }
  });
  backButton?.addEventListener("click", () => {
    if (!busy) callbacks.onBack();
  });
  syncControls();
}

function frequencyChoice(value: ScrubSetupFrequency, label: string, draft: ScrubSetupDraft, busy: boolean): string {
  return `<label><input name="scrub-setup-frequency" type="radio" value="${value}"${draft.automaticSchedule === value ? " checked" : ""}${busy || !draft.autoScrub ? " disabled" : ""}/><span>${label}</span></label>`;
}

function noticeChoice(value: ScrubSetupNotice, label: string, draft: ScrubSetupDraft, busy: boolean): string {
  return `<label><input name="scrub-setup-notice" type="radio" value="${value}"${draft.noticeSetting === value ? " checked" : ""}${busy ? " disabled" : ""}/><span>${label}</span></label>`;
}

function parseScrubSetupSummary(raw: unknown, command: ScrubSetupCommand): ScrubSetupSummary {
  if (!raw || typeof raw !== "object") throw new Error("Scrub setup returned an invalid summary");
  const candidate = raw as Record<string, unknown>;
  const accountCount = candidate.accountCount;
  const automaticSchedule = candidate.automaticSchedule;
  const noticeSetting = candidate.noticeSetting;
  if (!Number.isSafeInteger(accountCount) || (accountCount as number) < 0 || (accountCount as number) > 32) {
    throw new Error("Scrub setup returned an invalid account count");
  }
  if (automaticSchedule !== null && !isFrequency(automaticSchedule)) {
    throw new Error("Scrub setup returned an invalid schedule");
  }
  if (noticeSetting !== null && !isNotice(noticeSetting)) {
    throw new Error("Scrub setup returned an invalid notice setting");
  }
  const expectedCount = command.notNow ? 0 : command.selectedScanAccounts.length;
  const expectedSchedule = command.notNow ? null : command.automaticSchedule;
  const expectedNotice = command.notNow ? null : command.noticeSetting;
  if (accountCount !== expectedCount || automaticSchedule !== expectedSchedule || noticeSetting !== expectedNotice) {
    throw new Error("Scrub setup did not store the visible choices");
  }
  return { accountCount: accountCount as number, automaticSchedule, noticeSetting };
}

function copyDraft(draft: ScrubSetupDraft): ScrubSetupDraft {
  return { ...draft, selectedScanAccounts: new Set(draft.selectedScanAccounts) };
}

function isFrequency(value: unknown): value is ScrubSetupFrequency {
  return typeof value === "string" && (frequencies as readonly string[]).includes(value);
}

function isNotice(value: unknown): value is ScrubSetupNotice {
  return typeof value === "string" && (notices as readonly string[]).includes(value);
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
