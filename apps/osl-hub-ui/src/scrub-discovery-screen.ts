// The canonical Scrub screen: an exposure DISCOVERY console.
//
// Source of truth: OSL-AUDITS/reference/design-export-2026-08-08/Scrub.dc.html
// and README.md "Screens / Views > 6. Scrub". Two columns:
//
//   LEFT  - ACCOUNTS (per-account allow ticks, real handles from
//           `list_scrub_accounts`), then MODE - Discovery (free) vs AutoScrub
//           (discovery + deletion, reviewed batches) carrying a purple PRO tag.
//   RIGHT - a streaming CONSOLE with Run discovery / Stop / Clear, log lines
//           ~700ms apart, and the footer promise "Never logs in for you ·
//           never reads saved passwords" plus a coverage count.
//
// HONESTY. The free tier DISCOVERS and deletes nothing, and this build ships
// no account reader at all, so the console never invents counts: the line
// generator takes a reader, the shipping reader returns null for every
// account, and a null read is reported as exactly that. The closing line
// states what actually happened, always including "nothing deleted". Danger
// escalates by plain-sentence checkbox (design rule 4): the deletion consent
// -- the checkbox-per-account page from scrub-consent-page.ts -- exists only
// on the AutoScrub (Pro) path. Discovery deletes nothing and carries no gate.
//
// This module deliberately reuses the previously-orphaned modules instead of
// re-implementing them: scrub-account-choice.ts (account tick state + the
// drawn tick), scrub-consent-page.ts (the AutoScrub risk-tick consent page),
// and scrub-finished-notice-results.ts (the finished-run record shown when
// "Keep a record of what was found" is on).

import "./scrub-discovery-screen.css";
import {
  choiceTick,
  initialScrubAccountChoiceState,
  isScrubAccountTicked,
  toggleScrubAccountTick,
  type ScrubAccountChoiceState,
  type ScrubAccountRow,
} from "./scrub-account-choice";
import {
  continueFromScrubConsentPage,
  pressScrubConsentTermsButton,
  scrubConsentPageMarkup,
  scrubConsentPageState,
  setScrubConsentRiskTick,
  type ScrubConsentInvoke,
  type ScrubConsentPageState,
} from "./scrub-consent-page";
import {
  noticeResultsMarkup,
  openNoticeResults,
  type ScrubFinishedNotice,
  type ScrubRunResults,
} from "./scrub-finished-notice-results";
import { isTauriRuntime } from "./preferences";

// ---------------------------------------------------------------------------
// Console line planning -- pure, so the exact wording is testable.
// ---------------------------------------------------------------------------

export type ConsoleTone = "info" | "fact" | "accent" | "safe" | "warn" | "danger" | "pro";

export interface PlannedConsoleLine {
  readonly text: string;
  readonly tone: ConsoleTone;
}

export interface DiscoveryAccountReadResult {
  readonly readable: number;
  readonly exposures: number;
}

/** Returns real counts for an account, or null when nothing can be read. */
export type DiscoveryAccountReader = (account: ScrubAccountRow) => DiscoveryAccountReadResult | null;

/**
 * The reader this build ships. There is no account-reading engine in the
 * shipping graph yet (the IMAP/hosted-session readers are deliberately
 * unreachable -- see scrub-reachability.test.ts), so every read is null and
 * the console says so instead of inventing item counts.
 */
export const shippingDiscoveryReader: DiscoveryAccountReader = () => null;

/** The exact line the PRO-locked AutoScrub row logs on the free tier. */
export const AUTOSCRUB_PRO_LOCK_LINE = "autoscrub is a Pro feature · discovery stays free";

export function discoveryDoneLine(totalExposures: number): string {
  return `done · ${totalExposures} public exposures found · nothing deleted · deletion is AutoScrub (Pro)`;
}

export interface DiscoveryRunPlan {
  readonly lines: readonly PlannedConsoleLine[];
  readonly totalExposures: number;
  readonly readCount: number;
  readonly tickedCount: number;
}

export function planDiscoveryRun(
  ticked: readonly ScrubAccountRow[],
  read: DiscoveryAccountReader,
): DiscoveryRunPlan {
  const lines: PlannedConsoleLine[] = [
    { text: "scrub start · discovery · local only", tone: "accent" },
  ];
  if (ticked.length === 0) {
    lines.push({ text: "no accounts allowed · allow at least one account first", tone: "warn" });
    lines.push({ text: "stopped · nothing was read", tone: "danger" });
    return { lines, totalExposures: 0, readCount: 0, tickedCount: 0 };
  }
  lines.push({ text: `accounts · ${ticked.length} allowed · read one at a time`, tone: "info" });
  let totalExposures = 0;
  let readCount = 0;
  for (const account of ticked) {
    const name = `${account.serviceId} · ${account.accountLabel}`;
    const result = read(account);
    if (result === null) {
      // No reader in this build: the truthful line, never a made-up count.
      lines.push({ text: `${name} · account reader is not in this build · nothing was read`, tone: "warn" });
      continue;
    }
    readCount += 1;
    totalExposures += result.exposures;
    lines.push({ text: `${name} · ${result.readable} items readable · credentials untouched`, tone: "info" });
    lines.push({ text: `${name} · ${result.exposures} exposures look public`, tone: "fact" });
  }
  lines.push({ text: discoveryDoneLine(totalExposures), tone: "safe" });
  return { lines, totalExposures, readCount, tickedCount: ticked.length };
}

// ---------------------------------------------------------------------------
// Screen state -- module-level so it survives the app's full re-renders.
// ---------------------------------------------------------------------------

interface ConsoleLine extends PlannedConsoleLine {
  readonly time: string;
}

type AccountsLoad = "idle" | "loading" | "loaded" | "unavailable" | "failed";

interface DiscoveryScreenState {
  accountsLoad: AccountsLoad;
  loadDetail: string;
  choice: ScrubAccountChoiceState;
  keepRecord: boolean;
  running: boolean;
  lines: ConsoleLine[];
  coverage: string;
  finishedRun: ScrubRunResults | null;
  /** Open only on the Pro AutoScrub path. Discovery never renders a gate. */
  consent: ScrubConsentPageState | null;
}

const state: DiscoveryScreenState = {
  accountsLoad: "idle",
  loadDetail: "",
  choice: initialScrubAccountChoiceState([]),
  keepRecord: true,
  running: false,
  lines: [],
  coverage: "",
  finishedRun: null,
  consent: null,
};

let runTimers: ReturnType<typeof setTimeout>[] = [];

export interface ScrubDiscoveryDeps {
  invoke: (command: string, payload?: Record<string, unknown>) => Promise<unknown>;
  requestRender: () => void;
  proActive: boolean;
}

/** Test hook: puts the screen back to its first-open shape. */
export function resetScrubDiscoveryScreen(): void {
  runTimers.forEach(clearTimeout);
  runTimers = [];
  state.accountsLoad = "idle";
  state.loadDetail = "";
  state.choice = initialScrubAccountChoiceState([]);
  state.keepRecord = true;
  state.running = false;
  state.lines = [];
  state.coverage = "";
  state.finishedRun = null;
  state.consent = null;
}

function consoleClock(): string {
  const now = new Date();
  return [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map((part) => String(part).padStart(2, "0"))
    .join(":");
}

function pushLine(line: PlannedConsoleLine): void {
  state.lines.push({ ...line, time: consoleClock() });
}

// ---------------------------------------------------------------------------
// Accounts -- real registry data or an honest empty state, never invented.
// ---------------------------------------------------------------------------

function ensureAccountsLoaded(deps: ScrubDiscoveryDeps): void {
  if (state.accountsLoad !== "idle") return;
  if (!isTauriRuntime()) {
    // Browser/dev session with no hub backend: say so rather than fabricate.
    state.accountsLoad = "unavailable";
    state.loadDetail = "This session has no OSL backend, so no connected accounts can be listed.";
    return;
  }
  state.accountsLoad = "loading";
  Promise.resolve(deps.invoke("list_scrub_accounts"))
    .then((rows) => {
      const list = Array.isArray(rows) ? (rows as ScrubAccountRow[]) : [];
      state.choice = initialScrubAccountChoiceState(list);
      state.accountsLoad = "loaded";
      deps.requestRender();
    })
    .catch((failure: unknown) => {
      state.accountsLoad = "failed";
      state.loadDetail = failure instanceof Error ? failure.message : String(failure);
      deps.requestRender();
    });
}

// ---------------------------------------------------------------------------
// Run / stop / clear
// ---------------------------------------------------------------------------

function tickedAccounts(): ScrubAccountRow[] {
  return state.choice.accounts.filter((account) => isScrubAccountTicked(state.choice, account.accountId));
}

function startDiscoveryRun(deps: ScrubDiscoveryDeps): void {
  if (state.running) return;
  const ticked = tickedAccounts();
  const plan = planDiscoveryRun(ticked, shippingDiscoveryReader);
  state.lines = [];
  state.coverage = "";
  state.finishedRun = null;
  state.running = true;
  runTimers = [];
  plan.lines.forEach((line, index) => {
    runTimers.push(setTimeout(() => {
      pushLine(line);
      if (index === plan.lines.length - 1) finishDiscoveryRun(plan, ticked);
      deps.requestRender();
    }, 500 + index * 700));
  });
  deps.requestRender();
}

function finishDiscoveryRun(plan: DiscoveryRunPlan, ticked: readonly ScrubAccountRow[]): void {
  state.running = false;
  runTimers = [];
  if (plan.tickedCount > 0) {
    // Coverage counts only accounts that were actually read; with no reader
    // in this build that is zero, and the footer says zero.
    state.coverage = `Checked ${plan.readCount} of ${state.choice.accounts.length} accounts`;
    if (state.keepRecord) {
      state.finishedRun = {
        runId: Date.now().toString(36),
        service: "Discovery",
        account: ticked.map((row) => row.accountLabel).join(", "),
        matchCount: plan.totalExposures,
      };
    }
  }
}

function stopDiscoveryRun(deps: ScrubDiscoveryDeps): void {
  runTimers.forEach(clearTimeout);
  runTimers = [];
  pushLine({ text: "halted · you pressed stop · nothing else will run", tone: "danger" });
  state.running = false;
  deps.requestRender();
}

function clearConsole(deps: ScrubDiscoveryDeps): void {
  state.lines = [];
  state.coverage = "";
  if (!state.keepRecord) state.finishedRun = null;
  deps.requestRender();
}

// ---------------------------------------------------------------------------
// AutoScrub (Pro) path -- the only place a consent gate exists.
// ---------------------------------------------------------------------------

function pressAutoScrub(deps: ScrubDiscoveryDeps): void {
  if (!deps.proActive) {
    // Free tier: the row does nothing except say so, in the console.
    pushLine({ text: AUTOSCRUB_PRO_LOCK_LINE, tone: "pro" });
    deps.requestRender();
    return;
  }
  // Pro tier: deletion consent is a plain-sentence checkbox per account
  // (design rule 4), served by the already-built consent page.
  const discordAccounts = tickedAccounts()
    .filter((account) => account.serviceId === "discord")
    .map((account) => ({ accountId: account.accountId, accountLabel: account.accountLabel }));
  state.consent = scrubConsentPageState(discordAccounts);
  deps.requestRender();
}

async function continueAutoScrubConsent(deps: ScrubDiscoveryDeps): Promise<void> {
  if (!state.consent) return;
  const result = await continueFromScrubConsentPage(state.consent, deps.invoke as ScrubConsentInvoke);
  if (!result.ok) {
    state.consent = { ...state.consent, notice: result.refusal };
    deps.requestRender();
    return;
  }
  state.consent = null;
  // Consent is recorded, but the deletion runner is not in this build, so the
  // console reports exactly that: a stored consent, zero deletions.
  pushLine({
    text: `consent recorded · ${result.agreedAccountIds.length} account${result.agreedAccountIds.length === 1 ? "" : "s"} · autoscrub runner is not in this build · nothing was deleted`,
    tone: "warn",
  });
  deps.requestRender();
}

// ---------------------------------------------------------------------------
// Markup
// ---------------------------------------------------------------------------

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function accountRowMarkup(account: ScrubAccountRow): string {
  const ticked = isScrubAccountTicked(state.choice, account.accountId);
  return `<label class="sd-row sd-account${ticked ? " ticked" : ""}" data-sd-account-row="${escapeHtml(account.accountId)}">
    <span class="sd-row-names"><strong class="sd-row-name">${escapeHtml(account.accountLabel)}</strong><small class="sd-row-desc">${escapeHtml(account.appOrBrowserLabel)}</small></span>
    <input class="sr-only" type="checkbox" data-sd-account="${escapeHtml(account.accountId)}"${ticked ? " checked" : ""} aria-label="Allow Scrub to read ${escapeHtml(account.accountLabel)}"/>
    ${choiceTick()}
  </label>`;
}

function accountsListMarkup(): string {
  if (state.accountsLoad === "loading" || state.accountsLoad === "idle") {
    return `<p class="sd-empty">Reading which accounts are connected…</p>`;
  }
  if (state.accountsLoad === "unavailable" || state.accountsLoad === "failed") {
    return `<p class="sd-empty" title="${escapeHtml(state.loadDetail)}">No account list · ${escapeHtml(state.loadDetail)}</p>`;
  }
  if (state.choice.accounts.length === 0) {
    return `<p class="sd-empty">No signed-in account here can be scrubbed yet. Connect a service and it appears here.</p>`;
  }
  return state.choice.accounts.map(accountRowMarkup).join("");
}

function modeMarkup(proActive: boolean): string {
  const autoScrubReason = proActive
    ? "The AutoScrub runner is not in this build yet · consent can be recorded, nothing can be deleted"
    : "AutoScrub is a Pro feature · discovery stays free";
  return `<h2 class="sd-section">MODE</h2>
  <div class="sd-row sd-mode selected" role="radio" aria-checked="true" title="Discovery is the only mode this build can run">
    <span class="sd-row-names"><strong class="sd-row-name">Discovery</strong><small class="sd-row-desc">Finds what of yours is already out there</small></span>
    ${choiceTick()}
  </div>
  <button class="sd-row sd-mode sd-pro-locked" type="button" data-sd-autoscrub aria-disabled="true" title="${escapeHtml(autoScrubReason)}">
    <span class="sd-row-names"><strong class="sd-row-name">AutoScrub</strong><small class="sd-row-desc">Discovery + deletion, reviewed batches</small></span>
    <span class="sd-pro-tag">PRO</span>
    ${choiceTick()}
  </button>
  <label class="sd-row sd-keep${state.keepRecord ? " ticked" : ""}">
    <span class="sd-row-names"><strong class="sd-row-name">Keep a record of what was found</strong></span>
    <input class="sr-only" type="checkbox" data-sd-keep-record${state.keepRecord ? " checked" : ""}/>
    ${choiceTick()}
  </label>`;
}

function consentMarkup(): string {
  if (!state.consent) return "";
  return `<div class="sd-consent">${scrubConsentPageMarkup(state.consent)}</div>`;
}

function consoleLinesMarkup(): string {
  if (state.lines.length === 0 && !state.running) {
    return `<div class="sd-idle">Idle. Run discovery to see what OSL can find. Nothing leaves this device.</div>`;
  }
  return state.lines
    .map((line) => `<div class="sd-line"><span class="sd-line-time">${line.time}</span><span class="sd-line-text tone-${line.tone}">${escapeHtml(line.text)}</span></div>`)
    .join("");
}

function finishedRecordMarkup(): string {
  if (!state.finishedRun || !state.keepRecord) return "";
  const run = state.finishedRun;
  const notice: ScrubFinishedNotice = {
    id: `scrub-finished-${run.runId}`,
    service: run.service,
    finishedAccount: run.account,
    nextAccount: null,
    matchCount: run.matchCount,
    title: "Discovery finished",
    detail: "Kept because “Keep a record of what was found” is on. Nothing was deleted.",
  };
  return `<div class="sd-record">${noticeResultsMarkup(openNoticeResults(notice, [run]))}</div>`;
}

export function scrubDiscoveryScreenMarkup(proActive: boolean): string {
  const statusText = state.running ? "running · attended · local only" : "idle";
  return `<section class="scrub-discovery" id="scrub-discovery" data-running="${state.running ? "true" : "false"}">
  <header class="sd-head">
    <h1 id="route-heading" tabindex="-1">Find what of yours is already out there</h1>
    <p class="sd-lead">Reads your own machine, builds the list. Deleting is separate, reviewed, and never the default.</p>
  </header>
  <div class="sd-columns">
    <div class="sd-left">
      <h2 class="sd-section sd-section-first">ACCOUNTS</h2>
      <div class="sd-accounts" role="group" aria-label="Accounts Scrub may read">${accountsListMarkup()}</div>
      ${modeMarkup(proActive)}
      ${consentMarkup()}
    </div>
    <div class="sd-right">
      <div class="sd-console">
        <div class="sd-console-head">
          <span class="sd-dot${state.running ? " live" : ""}" aria-hidden="true"></span>
          <span class="sd-console-label">CONSOLE</span>
          <span class="sd-console-status">${statusText}</span>
          <button class="sd-clear" type="button" data-sd-clear>Clear</button>
          <button class="sd-run${state.running ? " running" : ""}" type="button" data-sd-run>${state.running ? "Stop" : "Run discovery"}</button>
        </div>
        <div class="sd-log" data-sd-log aria-live="polite">${consoleLinesMarkup()}</div>
        <div class="sd-console-foot">
          <span class="sd-promise">Never logs in for you · never reads saved passwords</span>
          <span class="sd-coverage">${escapeHtml(state.coverage)}</span>
        </div>
      </div>
      ${finishedRecordMarkup()}
    </div>
  </div>
</section>`;
}

// ---------------------------------------------------------------------------
// Binding -- called after every app render; the DOM is fresh each time.
// ---------------------------------------------------------------------------

export function bindScrubDiscoveryScreen(root: ParentNode, deps: ScrubDiscoveryDeps): void {
  ensureAccountsLoaded(deps);
  root.querySelectorAll<HTMLInputElement>("[data-sd-account]").forEach((input) =>
    input.addEventListener("change", () => {
      state.choice = toggleScrubAccountTick(state.choice, input.dataset.sdAccount ?? "");
      deps.requestRender();
    }));
  root.querySelector<HTMLInputElement>("[data-sd-keep-record]")?.addEventListener("change", (event) => {
    state.keepRecord = (event.currentTarget as HTMLInputElement).checked;
    if (!state.keepRecord) state.finishedRun = null;
    deps.requestRender();
  });
  root.querySelector<HTMLButtonElement>("[data-sd-autoscrub]")?.addEventListener("click", () => pressAutoScrub(deps));
  root.querySelector<HTMLButtonElement>("[data-sd-run]")?.addEventListener("click", () => {
    if (state.running) stopDiscoveryRun(deps);
    else startDiscoveryRun(deps);
  });
  root.querySelector<HTMLButtonElement>("[data-sd-clear]")?.addEventListener("click", () => clearConsole(deps));
  // AutoScrub consent page controls (present only while the panel is open).
  root.querySelectorAll<HTMLInputElement>("[data-risk-tick]").forEach((input) =>
    input.addEventListener("change", () => {
      if (!state.consent) return;
      state.consent = setScrubConsentRiskTick(state.consent, input.dataset.riskTick ?? "", input.checked);
      deps.requestRender();
    }));
  root.querySelectorAll<HTMLElement>("[data-terms-button]").forEach((button) =>
    button.addEventListener("click", () => {
      if (!state.consent) return;
      state.consent = pressScrubConsentTermsButton(state.consent, button.dataset.termsButton ?? "");
      deps.requestRender();
    }));
  root.querySelector<HTMLButtonElement>("#scrub-consent-back")?.addEventListener("click", () => {
    state.consent = null;
    deps.requestRender();
  });
  root.querySelector<HTMLButtonElement>("#scrub-consent-continue")?.addEventListener("click", () => {
    void continueAutoScrubConsent(deps);
  });
  const log = root.querySelector<HTMLElement>("[data-sd-log]");
  if (log) log.scrollTop = log.scrollHeight;
}
