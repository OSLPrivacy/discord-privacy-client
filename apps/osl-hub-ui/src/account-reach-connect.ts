import {
  listHubFriendAccountReachChoices,
  setHubFriendAccountReachChoice,
  type FriendAccountReachChoice,
} from "./adapters";
import { accountReachListErrors, type AccountReachChoice } from "./account-reach-list";

/**
 * TASK 0245 - connect the account reach tick boxes.
 *
 * TASK 0244 drew the "What they can see" list; nothing it drew was wired to
 * anything. This module binds each drawn tick box to TASK 0240's per-friend
 * account reach commands, one box to one account:
 *
 *   tick box change -> set_hub_friend_account_reach_choice(personId, serviceId,
 *                      accountId, broadened)
 *   refresh         -> list_hub_friend_account_reach_choices(personId)
 *
 * The rule the whole module exists to keep: one tick box moves exactly one
 * owned account. The command carries only the service and account identifiers
 * of the row the person touched, so no second account can be moved by a
 * neighbour's tick, and the list is re-stated from the backend's own answer
 * rather than from what the screen assumed happened.
 *
 * The DOM is typed structurally (as ./protected-box-shortcuts.ts does) so the
 * binding can be exercised against rendered markup without a browser.
 */

/** The one tick box of one owned account. */
export interface AccountReachBox {
  checked: boolean;
  getAttribute(name: string): string | null;
  addEventListener(type: string, listener: (event: unknown) => void): void;
}

export interface AccountReachRoot {
  querySelectorAll(selector: string): Iterable<AccountReachBox>;
}

/** The two per-friend account reach commands, as the binding calls them. */
export interface AccountReachCommands {
  setChoice(
    personId: string,
    serviceId: string,
    accountId: string,
    broadened: boolean,
  ): Promise<FriendAccountReachChoice | null>;
  listChoices(personId: string): Promise<FriendAccountReachChoice[] | null>;
}

/** The real commands: TASK 0240's, through the checked adapters. */
export const HUB_ACCOUNT_REACH_COMMANDS: AccountReachCommands = {
  setChoice: setHubFriendAccountReachChoice,
  listChoices: listHubFriendAccountReachChoices,
};

export const ACCOUNT_REACH_BOX_SELECTOR = "[data-account-reach]";

/** What one tick box was asked to do, and what the backend answered. */
export interface AccountReachToggle {
  serviceId: string;
  accountId: string;
  /** The state the person ticked the box into. */
  broadened: boolean;
  /** The backend's own record, or null when it refused. */
  saved: FriendAccountReachChoice | null;
  /** True when the box was put back because the backend refused. */
  revertedTo?: boolean;
}

function boxAccount(box: AccountReachBox): { serviceId: string; accountId: string } | null {
  const accountId = box.getAttribute("data-account-reach");
  const serviceId = box.getAttribute("data-account-reach-service");
  if (!accountId || !serviceId) return null;
  return { serviceId, accountId };
}

/**
 * Save one tick. Exactly one command, carrying exactly this account. When the
 * backend refuses, the box goes back to where it was: a tick that did not save
 * must not keep claiming it did.
 */
export async function saveAccountReachToggle(
  personId: string,
  box: AccountReachBox,
  commands: AccountReachCommands = HUB_ACCOUNT_REACH_COMMANDS,
): Promise<AccountReachToggle | null> {
  const account = boxAccount(box);
  if (account === null) return null;
  const broadened = box.checked === true;
  const saved = await commands.setChoice(personId, account.serviceId, account.accountId, broadened);
  if (saved === null) {
    box.checked = !broadened;
    return { ...account, broadened, saved: null, revertedTo: !broadened };
  }
  box.checked = saved.broadened;
  return { ...account, broadened, saved };
}

/**
 * Bind every drawn tick box to the per-friend commands. Returns how many boxes
 * were connected, so a screen that drew rows and connected none is visible.
 * `onToggle` sees each completed save (including a refusal) -- the binding
 * itself never swallows the outcome.
 */
export function connectAccountReachList(
  root: AccountReachRoot,
  personId: string,
  options: {
    commands?: AccountReachCommands;
    onToggle?: (toggle: AccountReachToggle) => void;
  } = {},
): number {
  const commands = options.commands ?? HUB_ACCOUNT_REACH_COMMANDS;
  let connected = 0;
  for (const box of root.querySelectorAll(ACCOUNT_REACH_BOX_SELECTOR)) {
    if (boxAccount(box) === null) continue;
    connected += 1;
    box.addEventListener("change", () => {
      void saveAccountReachToggle(personId, box, commands).then((toggle) => {
        if (toggle !== null) options.onToggle?.(toggle);
      });
    });
  }
  return connected;
}

/**
 * The tick state of every drawn account according to a direct query. An
 * account the backend never recorded a choice for is not reached: absence is
 * read as off, never as on.
 */
export function accountReachStates(
  accounts: readonly AccountReachChoice[],
  choices: readonly FriendAccountReachChoice[] | null,
): Record<string, boolean> {
  const recorded = new Map<string, boolean>();
  for (const choice of choices ?? []) {
    recorded.set(`${choice.serviceId}:${choice.accountId}`, choice.broadened);
  }
  const states: Record<string, boolean> = {};
  for (const account of accounts) {
    states[account.accountId] = recorded.get(`${account.serviceId}:${account.accountId}`) ?? false;
  }
  return states;
}

/**
 * Re-state the drawn list from the backend's own answer. Returns the accounts
 * with their queried tick states, or null when the query was refused -- a list
 * that could not be read is not re-drawn as all-off.
 */
export async function refreshAccountReachList(
  personId: string,
  accounts: readonly AccountReachChoice[],
  commands: AccountReachCommands = HUB_ACCOUNT_REACH_COMMANDS,
): Promise<AccountReachChoice[] | null> {
  const errors = accountReachListErrors(accounts);
  if (errors.length > 0) {
    throw new Error(`OSL: account reach list is not usable: ${errors.join("; ")}`);
  }
  const choices = await commands.listChoices(personId);
  if (choices === null) return null;
  const states = accountReachStates(accounts, choices);
  return accounts.map((account) => ({ ...account, checked: states[account.accountId] }));
}
