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
 * This module still only draws. TASK 0245 connects the drawn tick boxes to
 * 0240's commands (`set_hub_friend_account_reach_choice`,
 * `list_hub_friend_account_reach_choices`) in ./account-reach-connect.ts --
 * the same draw/connect split TASK 0821 used for the Home protection panel.
 * What this module owes the connect step is a row that carries everything one
 * command needs: the service id as well as the account id.
 */

/** One owned account's reach choice for the friend the list is drawn for. */
export interface AccountReachChoice {
  /**
   * Which app the owned account belongs to. The backend keys a reach choice on
   * service AND account (`account_reach_storage_key`), so a row without its
   * service cannot be sent to the per-friend commands at all. TASK 0245 draws
   * it beside the account id so a tick box carries everything the command needs.
   */
  serviceId: string;
  accountId: string;
  label: string;
  /** Whether this owned account can currently be seen by the friend. */
  checked: boolean;
}

/**
 * The identifier rule the backend itself applies to a service or account id
 * (`validate_account_reach_component`): 1-128 bytes of ASCII letters, digits,
 * `-` or `_`. A row that breaks it can never be saved, so it is refused here
 * rather than drawn as a tick box that silently does nothing.
 */
const ACCOUNT_REACH_COMPONENT = /^[A-Za-z0-9_-]{1,128}$/u;

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
    if (!account.serviceId?.trim()) errors.push(`account reach entry ${index} has no service id`);
    if (account.accountId?.trim() && !ACCOUNT_REACH_COMPONENT.test(account.accountId)) {
      errors.push(`account reach entry ${index} account id cannot be saved: ${JSON.stringify(account.accountId)}`);
    }
    if (account.serviceId?.trim() && !ACCOUNT_REACH_COMPONENT.test(account.serviceId)) {
      errors.push(`account reach entry ${index} service id cannot be saved: ${JSON.stringify(account.serviceId)}`);
    }
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
    <input type="checkbox" data-account-reach="${escapeHtml(account.accountId)}" data-account-reach-service="${escapeHtml(account.serviceId)}" ${account.checked ? "checked" : ""}/>
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
