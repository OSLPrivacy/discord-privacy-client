// TASK 1408 - the Scrub consent page.
//
// This is the screen that stands between the account-choice screen (task 1402)
// and a Scrub run. It shows one panel per chosen account, a terms button per
// account, one risk tick per account, and Back / Continue.
//
// The rules it has to keep are the ones the hub already enforces natively:
//
//   * `save_discord_scrub_consent_facts` (task 1405) stores the five consent
//     facts -- real reading, careful scrolling, service rules, ban risk,
//     stopping -- for one chosen account. The sentences below are sent as
//     those facts, so what a person reads on screen is what gets stored.
//   * `get_service_terms_address` (task 1406) hands back one address per
//     service and never opens it. The terms button here follows that: pressing
//     it reveals the address as text, it does not navigate anywhere.
//   * `continue_discord_scrub_after_risk_agreement` (task 1407) refuses until
//     every selected account has agreed. This page refuses in the same shape,
//     with the same message, before any `invoke` is made -- so a caller that
//     skips the disabled button still cannot get past this module.
//
// Continue is controlled by exactly one thing: the risk tick on every chosen
// account. Pressing a terms button, revealing an address, pressing Back, or
// any other state on this page never makes Continue available.

import "./scrub-consent-page.css";

export const SCRUB_CONSENT_PAGE_TITLE = "Before Scrub touches your Discord";

/** The Discord terms address task 1406's `get_service_terms_address` returns. */
export const DISCORD_TERMS_ADDRESS = "https://discord.com/terms";

/**
 * The five consent facts, in the wire shape of `DiscordScrubConsentFacts`
 * (apps/osl-hub/src/preferences.rs). Every one is plain ASCII and under the
 * 256-byte native limit, so the page can store exactly the sentences it shows.
 */
export const SCRUB_CONSENT_SENTENCES = {
  realReading:
    "Real reading: Scrub signs in as you and reads this account's messages one by one. Deletions are permanent.",
  carefulScrolling:
    "Careful scrolling: Scrub scrolls slowly, like a person. A busy account can take hours, and closing the window stops it part-way.",
  serviceRules:
    "Service rules: deleting in bulk can break Discord's rules. You are asking OSL to act on your account under those rules.",
  banRisk:
    "Ban risk: Discord can limit or permanently ban this account for it. Nobody at OSL can undo a ban, and a banned account can lose everything on it.",
  stopping:
    "Stopping: you can stop at any time. What is already deleted stays deleted, and the rest is left untouched.",
} as const;

export type ScrubConsentFacts = typeof SCRUB_CONSENT_SENTENCES;

/** The exact tick that controls Continue. Nothing else on the page does. */
export function riskTickLabel(accountLabel: string): string {
  return `I have read the ban risk and I accept it for ${accountLabel}.`;
}

export interface ScrubConsentAccount {
  accountId: string;
  accountLabel: string;
}

export interface ScrubConsentAccountState extends ScrubConsentAccount {
  /** The risk tick. The only field that can change Continue. */
  riskAgreed: boolean;
  /** Set by the terms button. Deliberately never consulted by the gate. */
  termsAddressShown: boolean;
}

export interface ScrubConsentPageState {
  serviceId: string;
  serviceName: string;
  termsAddress: string;
  accounts: ScrubConsentAccountState[];
  /** Refusal text from the last blocked Continue, shown on the page. */
  notice: string | null;
}

export function scrubConsentPageState(
  accounts: readonly ScrubConsentAccount[],
  options: { serviceId?: string; serviceName?: string; termsAddress?: string } = {},
): ScrubConsentPageState {
  return {
    serviceId: options.serviceId ?? "discord",
    serviceName: options.serviceName ?? "Discord",
    termsAddress: options.termsAddress ?? DISCORD_TERMS_ADDRESS,
    accounts: accounts.map((account) => ({ ...account, riskAgreed: false, termsAddressShown: false })),
    notice: null,
  };
}

/** The risk tick. An unknown account id changes nothing. */
export function setScrubConsentRiskTick(
  state: ScrubConsentPageState,
  accountId: string,
  agreed: boolean,
): ScrubConsentPageState {
  if (!state.accounts.some((account) => account.accountId === accountId)) return state;
  return {
    ...state,
    notice: null,
    accounts: state.accounts.map((account) =>
      account.accountId === accountId ? { ...account, riskAgreed: agreed } : account),
  };
}

/**
 * The terms button. It reveals the stored address as text; it never opens,
 * navigates to, or hosts the service, which is the rule task 1406 set.
 */
export function pressScrubConsentTermsButton(
  state: ScrubConsentPageState,
  accountId: string,
): ScrubConsentPageState {
  if (!state.accounts.some((account) => account.accountId === accountId)) return state;
  return {
    ...state,
    accounts: state.accounts.map((account) =>
      account.accountId === accountId ? { ...account, termsAddressShown: true } : account),
  };
}

export function scrubConsentAccountsMissingRiskTick(state: ScrubConsentPageState): string[] {
  return state.accounts.filter((account) => !account.riskAgreed).map((account) => account.accountId);
}

/**
 * Continue is available only when every chosen account carries its risk tick.
 * With no chosen account there is nothing to consent to, so it stays closed.
 */
export function scrubConsentContinueAvailable(state: ScrubConsentPageState): boolean {
  return state.accounts.length > 0 && scrubConsentAccountsMissingRiskTick(state).length === 0;
}

/** The same refusal wording `continue_discord_scrub_after_risk_agreement` uses. */
export function scrubConsentRefusalMessage(state: ScrubConsentPageState): string {
  if (state.accounts.length === 0) return "Choose a Discord account before continuing";
  return "Risk agreement is required for every selected Discord account before continuing: missing="
    + scrubConsentAccountsMissingRiskTick(state).join(",");
}

export type ScrubConsentInvoke = (command: string, payload?: Record<string, unknown>) => Promise<unknown>;

export type ScrubConsentContinueResult =
  | { ok: true; agreedAccountIds: string[] }
  | { ok: false; refusal: string };

/**
 * The one way off this page. It re-checks the risk ticks itself, so calling it
 * directly -- without ever touching the Continue button -- is refused in the
 * same way, and no `invoke` is made at all.
 */
export async function continueFromScrubConsentPage(
  state: ScrubConsentPageState,
  invoke: ScrubConsentInvoke,
): Promise<ScrubConsentContinueResult> {
  if (!scrubConsentContinueAvailable(state)) {
    return { ok: false, refusal: scrubConsentRefusalMessage(state) };
  }
  // Only ticked accounts are ever saved. If this page's own gate were ever
  // wrong, the hub still sees an account with no agreement and refuses, so the
  // page can never talk the hub into a consent nobody gave.
  const agreedAccountIds = state.accounts.filter((account) => account.riskAgreed).map((account) => account.accountId);
  try {
    for (const accountId of agreedAccountIds) {
      await invoke("save_discord_scrub_consent_facts", {
        input: { accountId, facts: { ...SCRUB_CONSENT_SENTENCES } },
      });
    }
    await invoke("continue_discord_scrub_after_risk_agreement");
  } catch (error) {
    return { ok: false, refusal: error instanceof Error ? error.message : String(error) };
  }
  return { ok: true, agreedAccountIds };
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

function sentenceListMarkup(): string {
  return Object.values(SCRUB_CONSENT_SENTENCES)
    .map((sentence) => `<li>${escapeHtml(sentence)}</li>`)
    .join("");
}

function accountMarkup(state: ScrubConsentPageState, account: ScrubConsentAccountState, index: number): string {
  const id = escapeHtml(account.accountId);
  const label = escapeHtml(account.accountLabel);
  const address = escapeHtml(state.termsAddress);
  const service = escapeHtml(state.serviceName);
  const shown = account.termsAddressShown
    ? `<p class="scp-terms-address" id="scrub-consent-terms-address-${index}">${service} terms: <code>${address}</code> (nothing was opened; copy it into your browser yourself)</p>`
    : `<p class="scp-terms-address scp-terms-hidden" id="scrub-consent-terms-address-${index}">Press the button to see the address. OSL never opens it for you.</p>`;
  return `<li class="scp-account" id="scrub-consent-account-${index}" data-account-id="${id}">
    <h2 class="scp-account-name">${label}</h2>
    <p class="scp-account-id">${id}</p>
    <ul class="scp-risks">${sentenceListMarkup()}</ul>
    <button class="button ghost scp-terms" id="scrub-consent-terms-${index}" type="button" data-terms-button="${id}" data-terms-address="${address}" aria-pressed="${account.termsAddressShown ? "true" : "false"}">Read ${service}'s terms</button>
    ${shown}
    <label class="scp-risk-tick" for="scrub-consent-risk-${index}"><input type="checkbox" id="scrub-consent-risk-${index}" data-risk-tick="${id}"${account.riskAgreed ? " checked" : ""}> ${escapeHtml(riskTickLabel(account.accountLabel))}</label>
  </li>`;
}

export function scrubConsentPageMarkup(state: ScrubConsentPageState): string {
  const available = scrubConsentContinueAvailable(state);
  const missing = scrubConsentAccountsMissingRiskTick(state);
  const notice = state.notice
    ? `<p class="scp-notice" id="scrub-consent-notice" role="alert">${escapeHtml(state.notice)}</p>`
    : "";
  return `<section class="scrub-consent-page" id="scrub-consent-page" aria-labelledby="scrub-consent-page-title" data-continue-available="${available ? "true" : "false"}" data-missing-risk-ticks="${escapeHtml(missing.join(","))}">
  <header class="scp-header">
    <h1 id="scrub-consent-page-title">${escapeHtml(SCRUB_CONSENT_PAGE_TITLE)}</h1>
    <p class="scp-lead">Read this for each account you chose. Continue stays closed until you accept the ban risk for every one of them.</p>
  </header>
  <ol class="scp-accounts">${state.accounts.map((account, index) => accountMarkup(state, account, index)).join("")}</ol>
  ${notice}
  <footer class="scp-footer">
    <button class="button ghost scp-back" id="scrub-consent-back" type="button">Back</button>
    <button class="button primary scp-continue" id="scrub-consent-continue" type="button"${available ? "" : ' disabled aria-disabled="true"'}>Continue</button>
  </footer>
</section>`;
}

export interface ScrubConsentPageBinding {
  getState(): ScrubConsentPageState;
  render(): void;
  /** The direct call: no button press, same gate. */
  continueNow(): Promise<ScrubConsentContinueResult>;
}

export interface ScrubConsentPageDeps {
  invoke: ScrubConsentInvoke;
  onBack?: () => void;
  onContinued?: (result: ScrubConsentContinueResult) => void;
}

/**
 * Paints the page into `root` and wires the controls. Every control is bound
 * through one delegated listener, and only the risk tick can move the gate.
 */
export function bindScrubConsentPage(
  root: HTMLElement,
  initial: ScrubConsentPageState,
  deps: ScrubConsentPageDeps,
): ScrubConsentPageBinding {
  let state = initial;
  const render = () => {
    root.innerHTML = scrubConsentPageMarkup(state);
  };

  const continueNow = async (): Promise<ScrubConsentContinueResult> => {
    const result = await continueFromScrubConsentPage(state, deps.invoke);
    if (!result.ok) {
      state = { ...state, notice: result.refusal };
      render();
    }
    deps.onContinued?.(result);
    return result;
  };

  root.addEventListener("change", (event) => {
    const target = event.target as HTMLElement | null;
    const accountId = target?.getAttribute?.("data-risk-tick");
    if (!accountId) return;
    state = setScrubConsentRiskTick(state, accountId, (target as HTMLInputElement).checked);
    render();
  });

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const terms = target?.closest?.("[data-terms-button]") as HTMLElement | null;
    if (terms) {
      state = pressScrubConsentTermsButton(state, terms.getAttribute("data-terms-button") ?? "");
      render();
      return;
    }
    if (target?.closest?.("#scrub-consent-back")) {
      deps.onBack?.();
      return;
    }
    if (target?.closest?.("#scrub-consent-continue")) void continueNow();
  });

  render();
  return { getState: () => state, render, continueNow };
}
