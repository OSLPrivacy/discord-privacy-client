// TASK 1418. The ready-to-scan page is the last screen before a scan run
// starts: it shows what 1413/1415 (chosen rules) and the account approval
// flow already decided, plus the watch choice from 1416 and the optional
// file scan from 1417, and turns Start into a run plan built only from what
// is on screen. Nothing here invents a choice the user did not make -- Start
// reads the same state the markup drew.

export interface ReadyToScanAccountSelection {
  serviceId: string;
  accountId: string;
}

export interface ReadyToScanAccount extends ReadyToScanAccountSelection {
  /** Whether this approved account is included in the run. Defaults to true: it was already approved. */
  included: boolean;
}

export interface ReadyToScanRule {
  id: string;
  label: string;
  /** Whether this bad-message rule (1411/1412) is part of the run's scope. */
  selected: boolean;
}

/** Mirrors `OslRunViewChoice` (apps/osl-hub/src/run_choices.rs), chosen on an earlier screen (1416). */
export type ReadyToScanWatchView = "watch-live" | "run-in-background";

export interface ReadyToScanState {
  runId: string;
  accounts: ReadyToScanAccount[];
  rules: ReadyToScanRule[];
  watchView: ReadyToScanWatchView;
  /** Extra files added through the optional file scan (1417); not a substitute for account scans. */
  optionalFiles: string[];
}

export interface ReadyToScanInit {
  runId: string;
  accounts: ReadyToScanAccountSelection[];
  rules: Array<{ id: string; label: string }>;
  watchView: ReadyToScanWatchView;
}

export function initialReadyToScanState(init: ReadyToScanInit): ReadyToScanState {
  return {
    runId: init.runId,
    accounts: init.accounts.map((account) => ({ ...account, included: true })),
    rules: init.rules.map((rule) => ({ ...rule, selected: true })),
    watchView: init.watchView,
    optionalFiles: [],
  };
}

/** Toggles exactly one approved account row. Every other row is untouched. */
export function toggleReadyToScanAccount(
  state: ReadyToScanState,
  serviceId: string,
  accountId: string,
): ReadyToScanState {
  return {
    ...state,
    accounts: state.accounts.map((account) =>
      account.serviceId === serviceId && account.accountId === accountId
        ? { ...account, included: !account.included }
        : account),
  };
}

/** Toggles exactly one chosen-rule row. Every other row is untouched. */
export function toggleReadyToScanRule(state: ReadyToScanState, ruleId: string): ReadyToScanState {
  return {
    ...state,
    rules: state.rules.map((rule) => (rule.id === ruleId ? { ...rule, selected: !rule.selected } : rule)),
  };
}

export function addReadyToScanFile(state: ReadyToScanState, path: string): ReadyToScanState {
  const trimmed = path.trim();
  if (trimmed.length === 0 || state.optionalFiles.includes(trimmed)) return state;
  return { ...state, optionalFiles: [...state.optionalFiles, trimmed] };
}

export function removeReadyToScanFile(state: ReadyToScanState, path: string): ReadyToScanState {
  return { ...state, optionalFiles: state.optionalFiles.filter((file) => file !== path) };
}

export interface ReadyToScanRunPlan {
  runId: string;
  watchView: ReadyToScanWatchView;
  /** The approved accounts still ticked on screen -- one visible choice. */
  accountList: ReadyToScanAccountSelection[];
  /** The chosen-rule ids still ticked on screen -- one visible choice. */
  scope: string[];
  /** The optional file scan's file count -- one visible choice. */
  batchSize: number;
}

/** Start reads only what is currently drawn: no field here comes from anywhere but `state`. */
export function buildReadyToScanRunPlan(state: ReadyToScanState): ReadyToScanRunPlan {
  return {
    runId: state.runId,
    watchView: state.watchView,
    accountList: state.accounts
      .filter((account) => account.included)
      .map(({ serviceId, accountId }) => ({ serviceId, accountId })),
    scope: state.rules.filter((rule) => rule.selected).map((rule) => rule.id),
    batchSize: state.optionalFiles.length,
  };
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

function watchViewLabel(watchView: ReadyToScanWatchView): string {
  return watchView === "watch-live" ? "Watch live" : "Run in background";
}

function accountRowMarkup(account: ReadyToScanAccount): string {
  const key = `${account.serviceId}::${account.accountId}`;
  return `<li class="ready-to-scan-account-row" data-ready-to-scan-account="${escapeHtml(key)}">
    <label>
      <input type="checkbox" data-ready-to-scan-account-toggle="${escapeHtml(key)}" ${account.included ? "checked" : ""}/>
      <span class="ready-to-scan-account-id">${escapeHtml(account.serviceId)} / ${escapeHtml(account.accountId)}</span>
    </label>
  </li>`;
}

function ruleRowMarkup(rule: ReadyToScanRule): string {
  return `<li class="ready-to-scan-rule-row" data-ready-to-scan-rule="${escapeHtml(rule.id)}">
    <label>
      <input type="checkbox" data-ready-to-scan-rule-toggle="${escapeHtml(rule.id)}" ${rule.selected ? "checked" : ""}/>
      <span class="ready-to-scan-rule-label">${escapeHtml(rule.label)}</span>
    </label>
  </li>`;
}

function optionalFileRowMarkup(path: string): string {
  return `<li class="ready-to-scan-file-row" data-ready-to-scan-file="${escapeHtml(path)}">
    <span class="ready-to-scan-file-path">${escapeHtml(path)}</span>
    <button class="button compact" data-ready-to-scan-remove-file="${escapeHtml(path)}" type="button">Remove</button>
  </li>`;
}

export function readyToScanMarkup(state: ReadyToScanState): string {
  const accountRows = state.accounts.map(accountRowMarkup).join("");
  const ruleRows = state.rules.map(ruleRowMarkup).join("");
  const fileRows = state.optionalFiles.map(optionalFileRowMarkup).join("");

  return `<section class="ready-to-scan-screen" aria-labelledby="ready-to-scan-title">
    <h1 id="ready-to-scan-title" tabindex="-1">Ready to scan</h1>

    <section class="ready-to-scan-accounts" aria-labelledby="ready-to-scan-accounts-title">
      <h2 id="ready-to-scan-accounts-title">Approved accounts</h2>
      <ul class="ready-to-scan-account-list">${accountRows}</ul>
    </section>

    <section class="ready-to-scan-rules" aria-labelledby="ready-to-scan-rules-title">
      <h2 id="ready-to-scan-rules-title">Chosen rules</h2>
      <ul class="ready-to-scan-rule-list">${ruleRows}</ul>
    </section>

    <section class="ready-to-scan-watch" aria-labelledby="ready-to-scan-watch-title">
      <h2 id="ready-to-scan-watch-title">Watch</h2>
      <p class="ready-to-scan-watch-value" data-ready-to-scan-watch-view="${escapeHtml(state.watchView)}">${escapeHtml(watchViewLabel(state.watchView))}</p>
    </section>

    <section class="ready-to-scan-file-scan" aria-labelledby="ready-to-scan-file-scan-title">
      <h2 id="ready-to-scan-file-scan-title">Optional file scan</h2>
      <ul class="ready-to-scan-file-list">${fileRows}</ul>
      <form class="ready-to-scan-file-form" data-ready-to-scan-add-file-form>
        <label for="ready-to-scan-file-input">Add a file to scan</label>
        <input id="ready-to-scan-file-input" type="text" data-ready-to-scan-file-input autocomplete="off"/>
        <button class="button compact" type="submit">Add file</button>
      </form>
    </section>

    <div class="ready-to-scan-actions">
      <button class="button" data-ready-to-scan-home type="button">Home</button>
      <button class="button" data-ready-to-scan-back type="button">Back</button>
      <button class="button primary" data-ready-to-scan-start type="button">Start</button>
    </div>
  </section>`;
}

export interface ReadyToScanHandle {
  state(): ReadyToScanState;
}

export interface MountReadyToScanOptions {
  state: ReadyToScanState;
  onHome?: () => void;
  onBack?: () => void;
  onStart?: (plan: ReadyToScanRunPlan) => void;
}

export function mountReadyToScanScreen(
  root: HTMLElement,
  { state, onHome, onBack, onStart }: MountReadyToScanOptions,
): ReadyToScanHandle {
  let current = state;

  const draw = (): void => {
    root.innerHTML = readyToScanMarkup(current);
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target) return;

    if (target.matches("[data-ready-to-scan-home]")) {
      onHome?.();
      return;
    }
    if (target.matches("[data-ready-to-scan-back]")) {
      onBack?.();
      return;
    }
    if (target.matches("[data-ready-to-scan-start]")) {
      onStart?.(buildReadyToScanRunPlan(current));
      return;
    }
    const removeFile = target.dataset.readyToScanRemoveFile;
    if (removeFile !== undefined) {
      current = removeReadyToScanFile(current, removeFile);
      draw();
    }
  });

  root.addEventListener("change", (event) => {
    const target = event.target as HTMLInputElement | null;
    if (!target) return;
    const accountKey = target.dataset.readyToScanAccountToggle;
    if (accountKey !== undefined) {
      const [serviceId, accountId] = accountKey.split("::");
      current = toggleReadyToScanAccount(current, serviceId, accountId);
      draw();
      return;
    }
    const ruleId = target.dataset.readyToScanRuleToggle;
    if (ruleId !== undefined) {
      current = toggleReadyToScanRule(current, ruleId);
      draw();
    }
  });

  root.addEventListener("submit", (event) => {
    const form = event.target as HTMLElement | null;
    if (!form || !form.matches("[data-ready-to-scan-add-file-form]")) return;
    event.preventDefault();
    const input = root.querySelector<HTMLInputElement>("[data-ready-to-scan-file-input]");
    if (!input) return;
    current = addReadyToScanFile(current, input.value);
    draw();
  });

  draw();

  return {
    state: () => current,
  };
}
