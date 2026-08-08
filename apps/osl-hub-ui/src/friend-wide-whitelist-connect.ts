import { invoke } from "@tauri-apps/api/core";
import { recordBackendFailure } from "./backend-failure";
import { isTauriRuntime } from "./preferences";
import type { AccountReachChoice } from "./account-reach-list";
import {
  FRIEND_WHITELIST_EVERYWHERE_SELECTOR,
  FRIEND_WHITELIST_NOWHERE_SELECTOR,
} from "./ui-behavior";

/** TASK 0261: connect the friend-wide controls to their two Hub actions. */
export const FRIEND_WHITELIST_EVERYWHERE_COMMAND = "set_hub_friend_account_reach_everywhere";
export const FRIEND_WHITELIST_NOWHERE_COMMAND = "set_hub_friend_account_reach_nowhere";
export const FRIEND_ACCOUNT_REACH_LIST_COMMAND = "list_hub_friend_account_reach_choices";

export type FriendWideWhitelistAction = "everywhere" | "nowhere";

export interface FriendWideWhitelistBulkResult {
  action: FriendWideWhitelistAction;
  personId: string;
  accounts: readonly {
    personId: string;
    serviceId: string;
    accountId: string;
    accountLabel: string;
    allowed: boolean;
  }[];
  changedCount: number;
}

export interface FriendWideWhitelistCommands {
  setEverywhere(
    personId: string,
    accounts: readonly AccountReachChoice[],
  ): Promise<FriendWideWhitelistBulkResult | null>;
  setNowhere(
    personId: string,
    accounts: readonly AccountReachChoice[],
  ): Promise<FriendWideWhitelistBulkResult | null>;
  refresh(
    personId: string,
    accounts: readonly AccountReachChoice[],
  ): Promise<AccountReachChoice[] | null>;
}

interface FriendWideWhitelistControl {
  disabled: boolean;
  readonly dataset: {
    readonly whitelistEverywherePerson?: string;
    readonly whitelistNowherePerson?: string;
  };
  addEventListener(type: string, listener: (event: unknown) => void): void;
}

interface AccountReachTick {
  checked: boolean;
  getAttribute(name: string): string | null;
}

/** Structural DOM boundary, so the shipped binding can be driven by a fixture. */
export interface FriendWideWhitelistRoot {
  querySelectorAll(selector: string): Iterable<unknown>;
}

export interface FriendWideWhitelistPressResult {
  action: FriendWideWhitelistAction;
  refreshed: AccountReachChoice[] | null;
}

function safeComponent(value: unknown, max: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && value.length <= max
    && /^[A-Za-z0-9_-]+$/u.test(value);
}

function safePersonId(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && value.length <= 180
    && !/[<>\u0000-\u001f\u007f]/u.test(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function accountArguments(accounts: readonly AccountReachChoice[]): readonly {
  serviceId: string;
  accountId: string;
  accountLabel: string;
}[] | null {
  const seen = new Set<string>();
  const result: { serviceId: string; accountId: string; accountLabel: string }[] = [];
  for (const account of accounts) {
    const key = `${account.serviceId}:${account.accountId}`;
    if (!safeComponent(account.serviceId, 128)
      || !safeComponent(account.accountId, 128)
      || account.label.trim().length === 0
      || account.label.length > 80
      || seen.has(key)) return null;
    seen.add(key);
    result.push({ serviceId: account.serviceId, accountId: account.accountId, accountLabel: account.label });
  }
  return result.length > 0 ? result : null;
}

function parseBulkResult(raw: unknown, action: FriendWideWhitelistAction, personId: string): FriendWideWhitelistBulkResult | null {
  if (!isRecord(raw)
    || raw.action !== action
    || raw.personId !== personId
    || !Number.isSafeInteger(raw.changedCount)
    || Number(raw.changedCount) < 0
    || !Array.isArray(raw.accounts)) return null;
  const accounts: Array<FriendWideWhitelistBulkResult["accounts"][number]> = [];
  for (const entry of raw.accounts) {
    if (!isRecord(entry)
      || entry.personId !== personId
      || !safeComponent(entry.serviceId, 128)
      || !safeComponent(entry.accountId, 128)
      || typeof entry.accountLabel !== "string"
      || entry.accountLabel.length === 0
      || entry.accountLabel.length > 80
      || typeof entry.allowed !== "boolean") return null;
    accounts.push({
      personId,
      serviceId: entry.serviceId,
      accountId: entry.accountId,
      accountLabel: entry.accountLabel,
      allowed: entry.allowed,
    });
  }
  return { action, personId, accounts, changedCount: Number(raw.changedCount) };
}

async function invokeBulkAction(
  command: typeof FRIEND_WHITELIST_EVERYWHERE_COMMAND | typeof FRIEND_WHITELIST_NOWHERE_COMMAND,
  action: FriendWideWhitelistAction,
  personId: string,
  accounts: readonly AccountReachChoice[],
): Promise<FriendWideWhitelistBulkResult | null> {
  const payload = accountArguments(accounts);
  if (!isTauriRuntime() || !safePersonId(personId) || payload === null) return null;
  try {
    return parseBulkResult(await invoke<unknown>(command, { personId, accounts: payload }), action, personId);
  } catch (error) {
    recordBackendFailure(command, error);
    return null;
  }
}

function parseReachChoices(
  raw: unknown,
  personId: string,
  accounts: readonly AccountReachChoice[],
): AccountReachChoice[] | null {
  if (!Array.isArray(raw) || raw.length > 512) return null;
  const states = new Map<string, boolean>();
  for (const entry of raw) {
    if (!isRecord(entry)
      || entry.personId !== personId
      || !safeComponent(entry.serviceId, 128)
      || !safeComponent(entry.accountId, 128)
      || typeof entry.broadened !== "boolean") return null;
    states.set(`${entry.serviceId}:${entry.accountId}`, entry.broadened);
  }
  return accounts.map((account) => ({
    ...account,
    checked: states.get(`${account.serviceId}:${account.accountId}`) ?? false,
  }));
}

export const HUB_FRIEND_WIDE_WHITELIST_COMMANDS: FriendWideWhitelistCommands = {
  setEverywhere: (personId, accounts) => invokeBulkAction(
    FRIEND_WHITELIST_EVERYWHERE_COMMAND,
    "everywhere",
    personId,
    accounts,
  ),
  setNowhere: (personId, accounts) => invokeBulkAction(
    FRIEND_WHITELIST_NOWHERE_COMMAND,
    "nowhere",
    personId,
    accounts,
  ),
  refresh: async (personId, accounts) => {
    if (!isTauriRuntime() || !safePersonId(personId) || accountArguments(accounts) === null) return null;
    try {
      return parseReachChoices(
        await invoke<unknown>(FRIEND_ACCOUNT_REACH_LIST_COMMAND, { personId }),
        personId,
        accounts,
      );
    } catch (error) {
      recordBackendFailure(FRIEND_ACCOUNT_REACH_LIST_COMMAND, error);
      return null;
    }
  },
};

function controls(root: FriendWideWhitelistRoot, selector: string): FriendWideWhitelistControl[] {
  return [...root.querySelectorAll(selector)] as FriendWideWhitelistControl[];
}

/** Apply one authoritative refresh to every visible account checkbox. */
export function refreshVisibleAccountReachTicks(
  root: FriendWideWhitelistRoot,
  refreshed: readonly AccountReachChoice[],
): number {
  const states = new Map(refreshed.map((account) => [
    `${account.serviceId}:${account.accountId}`,
    account.checked,
  ]));
  let changed = 0;
  for (const tick of [...root.querySelectorAll("[data-account-reach]")] as AccountReachTick[]) {
    const serviceId = tick.getAttribute("data-account-reach-service");
    const accountId = tick.getAttribute("data-account-reach");
    const checked = states.get(`${serviceId}:${accountId}`);
    if (checked === undefined) continue;
    if (tick.checked !== checked) changed += 1;
    tick.checked = checked;
  }
  return changed;
}

/**
 * Run the named bulk action, then query the Hub and repaint all visible ticks
 * from that query. The action response is never treated as the refresh.
 */
export async function pressFriendWideWhitelist(
  action: FriendWideWhitelistAction,
  personId: string,
  accounts: readonly AccountReachChoice[],
  root: FriendWideWhitelistRoot,
  commands: FriendWideWhitelistCommands = HUB_FRIEND_WIDE_WHITELIST_COMMANDS,
): Promise<FriendWideWhitelistPressResult> {
  const result = action === "everywhere"
    ? await commands.setEverywhere(personId, accounts)
    : await commands.setNowhere(personId, accounts);
  if (result === null) return { action, refreshed: null };
  const refreshed = await commands.refresh(personId, accounts);
  if (refreshed !== null) refreshVisibleAccountReachTicks(root, refreshed);
  return { action, refreshed };
}

/** Connect both TASK 0260 controls. Returns the number of buttons connected. */
export function connectFriendWideWhitelistButtons(
  root: FriendWideWhitelistRoot,
  accounts: readonly AccountReachChoice[],
  options: {
    commands?: FriendWideWhitelistCommands;
    onComplete?: (result: FriendWideWhitelistPressResult) => void;
  } = {},
): number {
  const commands = options.commands ?? HUB_FRIEND_WIDE_WHITELIST_COMMANDS;
  const allControls = [
    ...controls(root, FRIEND_WHITELIST_EVERYWHERE_SELECTOR),
    ...controls(root, FRIEND_WHITELIST_NOWHERE_SELECTOR),
  ];
  const bind = (selector: string, action: FriendWideWhitelistAction): number => {
    let connected = 0;
    for (const control of controls(root, selector)) {
      const personId = action === "everywhere"
        ? control.dataset.whitelistEverywherePerson ?? ""
        : control.dataset.whitelistNowherePerson ?? "";
      if (!safePersonId(personId)) continue;
      connected += 1;
      control.addEventListener("click", () => {
        for (const peer of allControls) peer.disabled = true;
        void pressFriendWideWhitelist(action, personId, accounts, root, commands)
          .then((result) => options.onComplete?.(result))
          .finally(() => { for (const peer of allControls) peer.disabled = false; });
      });
    }
    return connected;
  };
  return bind(FRIEND_WHITELIST_EVERYWHERE_SELECTOR, "everywhere")
    + bind(FRIEND_WHITELIST_NOWHERE_SELECTOR, "nowhere");
}
