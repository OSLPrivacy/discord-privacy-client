import "./scrub-account-choice.css";
import { continueButton } from "./onboarding-controls";
import type { ScrubSetupStep } from "./setup-persistence";

/**
 * TASK 1402 -- the final setup step where the owner picks which signed-in
 * accounts Scrub is allowed to touch.
 *
 * The list comes from `list_scrub_accounts` (TASK 1400) and the tick is the
 * only thing that authorises an account: pressing Continue calls
 * `save_scrub_account_permissions` (TASK 1401) with the ticked ids and nothing
 * else, so an account the owner left unticked is never written. Nothing on
 * this screen starts ticked -- permission is opt in, one account at a time --
 * and Continue stays unavailable, and refuses a direct call, until at least
 * one account is ticked.
 *
 * `availableAccountIds` is sent alongside the selection because the backend
 * rejects a selected id that is not in the available list; it is the list the
 * owner was actually shown, not a wider one.
 */

/** One row of `list_scrub_accounts`' `ScrubAccountDescriptor`. */
export interface ScrubAccountRow {
  readonly serviceId: string;
  readonly accountId: string;
  readonly accountLabel: string;
  readonly appOrBrowserLabel: string;
}

export interface ScrubAccountChoiceState {
  readonly accounts: readonly ScrubAccountRow[];
  readonly tickedAccountIds: readonly string[];
}

/** The step of the `scrub` setup route this screen owns, and its neighbours. */
export const SCRUB_ACCOUNT_CHOICE_STEP: ScrubSetupStep = "accounts";
export const SCRUB_ACCOUNT_CHOICE_BACK_STEP: ScrubSetupStep = "intro";
export const SCRUB_ACCOUNT_CHOICE_NEXT_STEP: ScrubSetupStep = "options";

export const SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND = "save_scrub_account_permissions";

export interface ScrubAccountPermissionWrite {
  readonly availableAccountIds: readonly string[];
  readonly selectedAccountIds: readonly string[];
}

/** Mirrors `ScrubAccountPermissionRead` from the Rust command. */
export interface ScrubAccountPermissionRead {
  readonly accountIds: readonly string[];
}

export type ScrubAccountChoiceInvoke = (
  command: string,
  payload: ScrubAccountPermissionWrite,
) => Promise<ScrubAccountPermissionRead>;

export type ScrubAccountChoiceContinue =
  | {
    readonly outcome: "saved";
    readonly step: ScrubSetupStep;
    readonly write: ScrubAccountPermissionWrite;
    readonly savedAccountIds: readonly string[];
  }
  | {
    readonly outcome: "refused";
    readonly step: ScrubSetupStep;
    readonly reason: "no-account-ticked";
  };

/** Nothing is ticked for the owner: Scrub gets an account only by being given one. */
export function initialScrubAccountChoiceState(
  accounts: readonly ScrubAccountRow[],
): ScrubAccountChoiceState {
  return { accounts, tickedAccountIds: [] };
}

/**
 * Ticking is per account and independent: it never ticks or unticks a second
 * row, and an id that is not on screen cannot be ticked at all.
 */
export function toggleScrubAccountTick(
  state: ScrubAccountChoiceState,
  accountId: string,
): ScrubAccountChoiceState {
  if (!state.accounts.some((account) => account.accountId === accountId)) return state;
  const ticked = new Set(state.tickedAccountIds);
  if (ticked.has(accountId)) ticked.delete(accountId);
  else ticked.add(accountId);
  return {
    accounts: state.accounts,
    // Kept in the order the rows are listed in, so the written selection reads
    // the same way the screen does.
    tickedAccountIds: state.accounts
      .map((account) => account.accountId)
      .filter((id) => ticked.has(id)),
  };
}

export function isScrubAccountTicked(state: ScrubAccountChoiceState, accountId: string): boolean {
  return state.tickedAccountIds.includes(accountId);
}

export function canContinueFromScrubAccountChoice(state: ScrubAccountChoiceState): boolean {
  return state.tickedAccountIds.length > 0;
}

/** The exact `save_scrub_account_permissions` payload this screen would send. */
export function scrubAccountPermissionWrite(
  state: ScrubAccountChoiceState,
): ScrubAccountPermissionWrite {
  return {
    availableAccountIds: state.accounts.map((account) => account.accountId),
    selectedAccountIds: state.accounts
      .map((account) => account.accountId)
      .filter((accountId) => state.tickedAccountIds.includes(accountId)),
  };
}

/** Back leaves the selection alone and writes nothing. */
export function backFromScrubAccountChoice(): ScrubSetupStep {
  return SCRUB_ACCOUNT_CHOICE_BACK_STEP;
}

/**
 * The only write on this screen. One tick, one call, one account saved: the
 * unticked rows are not in the payload, so the backend never sees them as
 * chosen. With no tick at all this refuses without calling the backend, so the
 * disabled Continue button is not the only thing standing between an empty
 * choice and a permission write.
 */
export async function continueFromScrubAccountChoice(
  state: ScrubAccountChoiceState,
  invoke: ScrubAccountChoiceInvoke,
): Promise<ScrubAccountChoiceContinue> {
  if (!canContinueFromScrubAccountChoice(state)) {
    return { outcome: "refused", step: SCRUB_ACCOUNT_CHOICE_STEP, reason: "no-account-ticked" };
  }
  const write = scrubAccountPermissionWrite(state);
  const read = await invoke(SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND, write);
  return {
    outcome: "saved",
    step: SCRUB_ACCOUNT_CHOICE_NEXT_STEP,
    write,
    savedAccountIds: read.accountIds,
  };
}

/** Drawn rather than a font glyph so the tick keeps its weight at any zoom. */
export function choiceTick(): string {
  return `<svg class="osl-tick" viewBox="0 0 18 18" width="18" height="18" fill="none" aria-hidden="true">
      <rect class="osl-tick-box" x="0.75" y="0.75" width="16.5" height="16.5" stroke-width="1.5"/>
      <path class="osl-tick-mark" d="m4.5 9.25 3 3 6-6.5" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>`;
}

function accountRow(account: ScrubAccountRow, ticked: boolean): string {
  return `<label class="scrub-account-row${ticked ? " ticked" : ""}" data-account-id="${escapeHtml(account.accountId)}" data-service-id="${escapeHtml(account.serviceId)}" data-ticked="${ticked ? "yes" : "no"}">
      <input class="sr-only scrub-account-tick" type="checkbox" name="scrub-account" value="${escapeHtml(account.accountId)}" aria-label="${escapeHtml(account.accountLabel)}"${ticked ? " checked" : ""}/>
      ${choiceTick()}
      <span class="scrub-account-names">
        <strong class="scrub-account-label">${escapeHtml(account.accountLabel)}</strong>
        <small class="scrub-account-where">${escapeHtml(account.appOrBrowserLabel)}</small>
      </span>
    </label>`;
}

/**
 * The empty list is a real state: `list_scrub_accounts` returns nothing when no
 * supported account is signed in, and saying so is better than an empty box
 * above a Continue button that can never be pressed.
 */
export function scrubAccountChoiceMarkup(state: ScrubAccountChoiceState): string {
  const ready = canContinueFromScrubAccountChoice(state);
  const rows = state.accounts.length
    ? state.accounts
      .map((account) => accountRow(account, isScrubAccountTicked(state, account.accountId)))
      .join("\n      ")
    : `<p class="scrub-account-empty">No signed-in account here can be scrubbed yet.</p>`;

  return `<section class="scrub-account-screen" data-setup-route="scrub" data-setup-step="${SCRUB_ACCOUNT_CHOICE_STEP}" data-ticked-count="${state.tickedAccountIds.length}">
  <header class="scrub-account-head">
    <h1 class="scrub-account-title">Which accounts can Scrub use?</h1>
    <p class="scrub-account-lead">Tick an account to let Scrub work on it. Anything you leave unticked is not saved and Scrub never touches it.</p>
  </header>
  <div class="scrub-account-list" role="group" aria-label="Accounts Scrub can use">
      ${rows}
  </div>
  <div class="scrub-account-footer setup-footer onboarding-actions">
    <button class="button ghost scrub-account-back" id="scrub-accounts-back" data-setup-back="${SCRUB_ACCOUNT_CHOICE_BACK_STEP}" type="button">Back</button>
    ${continueButton(`id="scrub-accounts-continue" data-setup-continue="${SCRUB_ACCOUNT_CHOICE_NEXT_STEP}"${ready ? "" : ' disabled aria-disabled="true"'}`, "scrub-account-continue")}
  </div>
</section>`;
}

/**
 * The `scrub` setup route's step router. Only the accounts step belongs to this
 * module; the intro and options steps stay with whoever owns them, so an
 * unknown step renders nothing here rather than guessing a screen.
 */
export function scrubSetupStepMarkup(step: ScrubSetupStep, state: ScrubAccountChoiceState): string {
  return step === SCRUB_ACCOUNT_CHOICE_STEP ? scrubAccountChoiceMarkup(state) : "";
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
