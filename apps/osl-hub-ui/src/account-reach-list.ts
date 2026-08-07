/**
 * TASK 0244 - the "What they can see" account reach tick-box list.
 *
 * TASK 0240 (gate) added `SecurityPreferences.friend_account_reach_choices` on
 * the backend: an encrypted per-friend, per-owned-account boolean, so two
 * different owned accounts can carry two different choices for the same
 * friend. This module draws that fact as a list a person can read and tick:
 * one row per owned account, each with its own checkbox reflecting whether
 * that account currently reaches the friend.
 *
 * This is a drawing task only. Nothing here calls the Tauri commands from
 * 0240 (`set_hub_friend_account_reach_choice`, `list_hub_friend_account_reach_choices`);
 * wiring the list to those commands is left to a later "connect" task, the
 * same split TASK 0821 used for the Home protection panel.
 */

/** One owned account's reach choice for the friend the list is drawn for. */
export interface AccountReachChoice {
  accountId: string;
  label: string;
  /** Whether this owned account can currently be seen by the friend. */
  checked: boolean;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

/** Every way the account reach data contradicts itself. Empty means drawable. */
export function accountReachListErrors(accounts: readonly AccountReachChoice[]): string[] {
  const errors: string[] = [];
  if (!Array.isArray(accounts)) {
    errors.push(`account reach list is missing: ${JSON.stringify(accounts)}`);
    return errors;
  }
  const seen = new Set<string>();
  accounts.forEach((account, index) => {
    if (!account.accountId?.trim()) errors.push(`account reach entry ${index} has no account id`);
    if (!account.label?.trim()) errors.push(`account reach entry ${index} has no label`);
    if (typeof account.checked !== "boolean") {
      errors.push(`account reach entry ${index} tick state is not on or off: ${JSON.stringify(account.checked)}`);
    }
    if (account.accountId && seen.has(account.accountId)) {
      errors.push(`account reach entry ${index} repeats account id ${account.accountId}`);
    }
    if (account.accountId) seen.add(account.accountId);
  });
  return errors;
}

function accountReachRow(account: AccountReachChoice): string {
  return `<label class="account-reach-item" data-account-reach-item="${escapeHtml(account.accountId)}">
    <span>${escapeHtml(account.label)}</span>
    <input type="checkbox" data-account-reach="${escapeHtml(account.accountId)}" ${account.checked ? "checked" : ""}/>
  </label>`;
}

/**
 * The "What they can see" list: one tick box per owned account, reflecting
 * that account's current reach choice for the friend the list is drawn for.
 * Throws when the data contradicts itself -- a duplicate account or a tick
 * state that is neither on nor off is worse than a list drawn calmly.
 */
export function accountReachListMarkup(accounts: readonly AccountReachChoice[]): string {
  const errors = accountReachListErrors(accounts);
  if (errors.length > 0) {
    throw new Error(`OSL: account reach list is not usable: ${errors.join("; ")}`);
  }
  if (accounts.length === 0) {
    return `<fieldset class="account-reach-sources" data-account-reach-list data-account-count="0"><legend>What they can see</legend><p class="account-reach-empty">No owned accounts yet</p></fieldset>`;
  }
  const rows = accounts.map(accountReachRow).join("");
  return `<fieldset class="account-reach-sources" data-account-reach-list data-account-count="${accounts.length}"><legend>What they can see</legend><div class="account-reach-list">${rows}</div></fieldset>`;
}
