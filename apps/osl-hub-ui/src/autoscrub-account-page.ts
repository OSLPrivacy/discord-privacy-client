import { invoke } from "@tauri-apps/api/core";

/**
 * Task 1456: connect the AutoScrub account page.
 *
 * The account page has four things to show: why AutoScrub is paid (the Pro
 * explanation), the Unlock Pro call to action, the Enter Pro code form, and
 * the list of approved-account switches from Task 1455
 * (`crates/ipc/src/autoscrub_account_switches.rs`). Every function here talks
 * to that module's three direct-invoke commands by the exact names it owns —
 * `autoscrub_account_switches`, `autoscrub_account_switch`,
 * `autoscrub_present_pro_code` — and never reshapes a refusal into a success.
 */

export const AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND = "autoscrub_account_switches";
export const AUTOSCRUB_ACCOUNT_SWITCH_COMMAND = "autoscrub_account_switch";
export const AUTOSCRUB_PRESENT_PRO_CODE_COMMAND = "autoscrub_present_pro_code";

/** Same value as `crates/ipc/src/autoscrub_pro_gate.rs::PRO_CODE_REQUIRED`. */
const PRO_CODE_REQUIRED = "pro_code_required";

export interface AutoScrubSwitchRefusal {
  readonly command: string;
  readonly accountId: string;
  readonly reason: string;
  readonly message: string;
}

export interface AutoScrubAccountSwitchRow {
  readonly accountId: string;
  readonly label: string;
  readonly available: boolean;
  readonly on: boolean;
  readonly refusal?: AutoScrubSwitchRefusal;
}

export interface AutoScrubAccountRecord {
  readonly accountId: string;
  readonly label: string;
  readonly on: boolean;
}

export interface AutoScrubAccountSwitchListing {
  readonly switches: readonly AutoScrubAccountSwitchRow[];
  readonly recordCount: number;
}

export interface AutoScrubAccountSwitchOutcome {
  readonly accountId: string;
  readonly on: boolean;
  readonly records: readonly AutoScrubAccountRecord[];
  readonly recordCount: number;
}

export interface AutoScrubPresentProCodeResult {
  readonly verdict: string;
  readonly unlocked: boolean;
  readonly switches: readonly AutoScrubAccountSwitchRow[];
  readonly recordCount: number;
}

export type AutoScrubAccountCommandReply<T> =
  | { readonly ok: true; readonly command: string; readonly result: T }
  | {
      readonly ok: false;
      readonly command: string;
      readonly errorCode: string;
      readonly error: string;
      readonly accountId?: string;
    };

type NativeInvoke = typeof invoke;

async function callAutoScrubAccountCommand<T>(
  command: string,
  args: Record<string, unknown>,
  nativeInvoke: NativeInvoke,
): Promise<AutoScrubAccountCommandReply<T>> {
  return nativeInvoke<AutoScrubAccountCommandReply<T>>(command, args);
}

/** The listing a screen renders: every row, plus how many records are saved. */
export async function loadAutoScrubAccountSwitches(
  nativeInvoke: NativeInvoke = invoke,
): Promise<AutoScrubAccountCommandReply<AutoScrubAccountSwitchListing>> {
  return callAutoScrubAccountCommand(AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND, {}, nativeInvoke);
}

/**
 * Move one account's AutoScrub switch. Refused before anything changes if the
 * account is not the exact one normal Scrub consent approved (Task 1455) or
 * AutoScrub is still Pro-locked (Task 1454) — the reply's `ok` field always
 * tells the caller which happened, so a refusal is never mistaken for success.
 */
export async function setAutoScrubAccountSwitch(
  accountId: string,
  on: boolean,
  nativeInvoke: NativeInvoke = invoke,
): Promise<AutoScrubAccountCommandReply<AutoScrubAccountSwitchOutcome>> {
  return callAutoScrubAccountCommand(
    AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
    { request: { accountId, on } },
    nativeInvoke,
  );
}

/** Presents a Pro code straight at this page's own Pro gate. */
export async function presentAutoScrubProCode(
  code: string,
  nativeInvoke: NativeInvoke = invoke,
): Promise<AutoScrubAccountCommandReply<AutoScrubPresentProCodeResult>> {
  return callAutoScrubAccountCommand(
    AUTOSCRUB_PRESENT_PRO_CODE_COMMAND,
    { request: { code } },
    nativeInvoke,
  );
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Why AutoScrub is the paid half of Scrub, shown above the switches. */
export function autoScrubProExplanationMarkup(unlocked: boolean): string {
  return `<section class="autoscrub-pro-explanation" data-autoscrub-pro-unlocked="${unlocked}"><h2>AutoScrub</h2><p>Free Scrub finds and reviews. AutoScrub is the paid half: it schedules, records, and can delete on the accounts you approve. It runs only on accounts approved in the Scrub account list, and only while an active Pro code is held.</p></section>`;
}

/** The call to action shown while AutoScrub is still Pro-locked. */
export function autoScrubUnlockProMarkup(unlocked: boolean): string {
  if (unlocked) {
    return `<p class="autoscrub-pro-status" data-autoscrub-pro-unlocked="true">Pro is unlocked. AutoScrub account switches are available for every approved account.</p>`;
  }
  return `<div class="autoscrub-unlock-pro" data-autoscrub-pro-unlocked="false"><p>AutoScrub account switches need an active Pro code.</p><button class="button primary" id="autoscrub-unlock-pro" type="button">Unlock Pro</button></div>`;
}

/** The Enter Pro code form, submitted straight at `autoscrub_present_pro_code`. */
export function autoScrubEnterProCodeFormMarkup(): string {
  return `<form id="autoscrub-pro-code-form" class="pro-setup-form pro-code-form" novalidate><label class="sr-only" for="autoscrub-pro-code-input">Pro activation code</label><input id="autoscrub-pro-code-input" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="signin-unlock" type="submit"><span class="signin-unlock-label">Enter Pro code</span></button></form>`;
}

/** One approved-account switch row: greyed out and reasoned when unavailable. */
export function autoScrubAccountSwitchRowMarkup(row: AutoScrubAccountSwitchRow): string {
  const reasonAttr = row.refusal ? ` data-autoscrub-refusal-reason="${escapeHtml(row.refusal.reason)}"` : "";
  const reasonText = row.refusal ? `<small>${escapeHtml(row.refusal.message)}</small>` : "";
  return `<div class="autoscrub-account-switch-row" data-account-id="${escapeHtml(row.accountId)}" data-available="${row.available}"${reasonAttr}><span><strong>${escapeHtml(row.label)}</strong><small>${escapeHtml(row.accountId)}</small>${reasonText}</span><input type="checkbox" class="autoscrub-account-switch" data-account-id="${escapeHtml(row.accountId)}" ${row.on ? "checked" : ""} ${row.available ? "" : "disabled"}/></div>`;
}

/** The full approved-account switches list, plus the Pro explanation and gate. */
export function autoScrubAccountPageMarkup(listing: AutoScrubAccountSwitchListing, unlocked: boolean): string {
  const rows = listing.switches.map((row) => autoScrubAccountSwitchRowMarkup(row)).join("");
  return `<section class="autoscrub-account-page">${autoScrubProExplanationMarkup(unlocked)}${autoScrubUnlockProMarkup(unlocked)}${unlocked ? "" : autoScrubEnterProCodeFormMarkup()}<div class="autoscrub-account-switches" data-record-count="${listing.recordCount}">${rows}</div></section>`;
}

/**
 * The live state the account page screen holds: the last-loaded listing,
 * whether Pro is unlocked, and the most recent refusal (if any) so a screen
 * can show it without inventing its own copy.
 */
export interface AutoScrubAccountPageState {
  readonly listing: AutoScrubAccountSwitchListing;
  readonly unlocked: boolean;
  readonly error?: string;
}

export function autoScrubAccountPageInitialState(): AutoScrubAccountPageState {
  return { listing: { switches: [], recordCount: 0 }, unlocked: false };
}

/** `true` iff a listing shows every row unlocked (none refused for the Pro gate). */
function listingIsProUnlocked(listing: AutoScrubAccountSwitchListing): boolean {
  return !listing.switches.some((row) => row.refusal?.reason === PRO_CODE_REQUIRED);
}

/** Loads the switch listing fresh from the backend and replaces the state with it. */
export async function autoScrubAccountPageLoad(
  state: AutoScrubAccountPageState,
  nativeInvoke: NativeInvoke = invoke,
): Promise<AutoScrubAccountPageState> {
  const reply = await loadAutoScrubAccountSwitches(nativeInvoke);
  if (!reply.ok) {
    return { ...state, error: reply.error };
  }
  return { listing: reply.result, unlocked: listingIsProUnlocked(reply.result), error: undefined };
}

/**
 * Moves one account's switch through the backend and folds the outcome back
 * into state. A refusal never mutates `listing`/`unlocked` -- the same
 * fail-closed guarantee `setAutoScrubAccountSwitch` documents -- it only sets
 * `error` so a screen can show why the row snapped back.
 */
export async function autoScrubAccountPageToggleSwitch(
  state: AutoScrubAccountPageState,
  accountId: string,
  on: boolean,
  nativeInvoke: NativeInvoke = invoke,
): Promise<AutoScrubAccountPageState> {
  const reply = await setAutoScrubAccountSwitch(accountId, on, nativeInvoke);
  if (!reply.ok) {
    return { ...state, error: reply.error };
  }
  const switches = state.listing.switches.map((row) =>
    row.accountId === accountId ? { ...row, on: reply.result.on } : row,
  );
  return {
    listing: { switches, recordCount: reply.result.recordCount },
    unlocked: state.unlocked,
    error: undefined,
  };
}

/** Presents a Pro code and folds the resulting listing/unlock state back in. */
export async function autoScrubAccountPagePresentCode(
  state: AutoScrubAccountPageState,
  code: string,
  nativeInvoke: NativeInvoke = invoke,
): Promise<AutoScrubAccountPageState> {
  const reply = await presentAutoScrubProCode(code, nativeInvoke);
  if (!reply.ok) {
    return { ...state, error: reply.error };
  }
  return {
    listing: { switches: reply.result.switches, recordCount: reply.result.recordCount },
    unlocked: reply.result.unlocked,
    error: undefined,
  };
}

/**
 * Wires a rendered `autoScrubAccountPageMarkup` container to the backend:
 * checkbox changes call `autoscrub_account_switch`, the Pro code form calls
 * `autoscrub_present_pro_code`, and the Unlock Pro button focuses that form's
 * input. Every mutation re-renders the container from the fresh state, so the
 * switch and record count a screen shows always match what was saved.
 */
export function connectAutoScrubAccountPage(
  container: { innerHTML: string; querySelector: (selector: string) => unknown; querySelectorAll: (selector: string) => Iterable<unknown> },
  getState: () => AutoScrubAccountPageState,
  setState: (next: AutoScrubAccountPageState) => void,
  nativeInvoke: NativeInvoke = invoke,
): void {
  const render = () => {
    const state = getState();
    container.innerHTML = autoScrubAccountPageMarkup(state.listing, state.unlocked);
    for (const element of container.querySelectorAll(".autoscrub-account-switch")) {
      const input = element as { addEventListener: (type: string, listener: (event: Event) => void) => void; dataset: { accountId?: string }; checked: boolean };
      input.addEventListener("change", (event) => {
        const accountId = input.dataset.accountId ?? "";
        const on = (event.target as { checked: boolean }).checked;
        void autoScrubAccountPageToggleSwitch(getState(), accountId, on, nativeInvoke).then((next) => {
          setState(next);
          render();
        });
      });
    }
    const unlockButton = container.querySelector("#autoscrub-unlock-pro") as
      | { addEventListener: (type: string, listener: () => void) => void }
      | null;
    unlockButton?.addEventListener("click", () => {
      (container.querySelector("#autoscrub-pro-code-input") as { focus?: () => void } | null)?.focus?.();
    });
    const form = container.querySelector("#autoscrub-pro-code-form") as
      | { addEventListener: (type: string, listener: (event: Event) => void) => void }
      | null;
    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      const input = container.querySelector("#autoscrub-pro-code-input") as { value: string } | null;
      const code = input?.value ?? "";
      void autoScrubAccountPagePresentCode(getState(), code, nativeInvoke).then((next) => {
        setState(next);
        render();
      });
    });
  };
  render();
}
