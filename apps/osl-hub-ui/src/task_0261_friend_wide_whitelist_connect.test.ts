import { describe, expect, it } from "vitest";
import { accountReachListMarkup, type AccountReachChoice } from "./account-reach-list";
import {
  connectFriendWideWhitelistButtons,
  FRIEND_ACCOUNT_REACH_LIST_COMMAND,
  FRIEND_WHITELIST_EVERYWHERE_COMMAND,
  FRIEND_WHITELIST_NOWHERE_COMMAND,
  type FriendWideWhitelistCommands,
  type FriendWideWhitelistPressResult,
} from "./friend-wide-whitelist-connect";
import {
  friendWhitelistEverywhereButtonMarkup,
  friendWhitelistNowhereButtonMarkup,
} from "./ui-behavior";

const PERSON_ID = "hub-person-task-0261";
const ACCOUNTS: AccountReachChoice[] = [
  { serviceId: "discord", accountId: "discord-main", label: "Discord main", checked: false },
  { serviceId: "telegram", accountId: "telegram-work", label: "Telegram work", checked: false },
  { serviceId: "signal", accountId: "signal-private", label: "Signal private", checked: false },
];

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

class FixtureControl {
  disabled = false;
  readonly dataset: { whitelistEverywherePerson?: string; whitelistNowherePerson?: string };
  private readonly listeners: (() => void)[] = [];

  constructor(attribute: string, personId: string) {
    this.dataset = attribute === "data-whitelist-everywhere-person"
      ? { whitelistEverywherePerson: personId }
      : { whitelistNowherePerson: personId };
  }

  addEventListener(type: string, listener: () => void): void {
    if (type === "click") this.listeners.push(listener);
  }

  press(): void {
    expect(this.disabled).toBe(false);
    for (const listener of this.listeners) listener();
  }
}

class FixtureTick {
  checked: boolean;
  constructor(
    private readonly serviceId: string,
    private readonly accountId: string,
    checked: boolean,
  ) {
    this.checked = checked;
  }
  getAttribute(name: string): string | null {
    if (name === "data-account-reach-service") return this.serviceId;
    if (name === "data-account-reach") return this.accountId;
    return null;
  }
}

function fixture(): {
  markup: string;
  everywhere: FixtureControl;
  nowhere: FixtureControl;
  ticks: FixtureTick[];
  root: { querySelectorAll(selector: string): Iterable<unknown> };
} {
  const markup = `<article class="person-row person-profile"><div class="friend-whitelist-reach">${friendWhitelistEverywhereButtonMarkup(PERSON_ID, escapeHtml)}${friendWhitelistNowhereButtonMarkup(PERSON_ID, escapeHtml)}</div>${accountReachListMarkup(ACCOUNTS)}</article>`;
  const everywhere = new FixtureControl("data-whitelist-everywhere-person", PERSON_ID);
  const nowhere = new FixtureControl("data-whitelist-nowhere-person", PERSON_ID);
  const ticks = [...markup.matchAll(/data-account-reach="([^"]+)" data-account-reach-service="([^"]+)" (checked)?/gu)]
    .map((match) => new FixtureTick(match[2] ?? "", match[1] ?? "", match[3] === "checked"));
  const root = {
    querySelectorAll(selector: string): Iterable<unknown> {
      if (selector === "[data-whitelist-everywhere-person]") return [everywhere];
      if (selector === "[data-whitelist-nowhere-person]") return [nowhere];
      if (selector === "[data-account-reach]") return ticks;
      return [];
    },
  };
  return { markup, everywhere, nowhere, ticks, root };
}

function fakeHub(
  commandLog: string[],
  received: { action: "everywhere" | "nowhere"; accountIds: string[] }[],
): FriendWideWhitelistCommands {
  const saved = new Map(ACCOUNTS.map((account) => [
    `${account.serviceId}:${account.accountId}`,
    account.checked,
  ]));
  const apply = async (action: "everywhere" | "nowhere") => {
    commandLog.push(action === "everywhere" ? FRIEND_WHITELIST_EVERYWHERE_COMMAND : FRIEND_WHITELIST_NOWHERE_COMMAND);
    const allowed = action === "everywhere";
    let changedCount = 0;
    for (const key of saved.keys()) {
      if (saved.get(key) !== allowed) changedCount += 1;
      saved.set(key, allowed);
    }
    return {
      action,
      personId: PERSON_ID,
      accounts: ACCOUNTS.map((account) => ({
        personId: PERSON_ID,
        serviceId: account.serviceId,
        accountId: account.accountId,
        accountLabel: account.label,
        allowed,
      })),
      changedCount,
    };
  };
  return {
    setEverywhere: async (personId, accounts) => {
      expect(personId).toBe(PERSON_ID);
      received.push({ action: "everywhere", accountIds: accounts.map((account) => account.accountId) });
      return apply("everywhere");
    },
    setNowhere: async (personId, accounts) => {
      expect(personId).toBe(PERSON_ID);
      received.push({ action: "nowhere", accountIds: accounts.map((account) => account.accountId) });
      return apply("nowhere");
    },
    refresh: async (personId, accounts) => {
      expect(personId).toBe(PERSON_ID);
      commandLog.push(FRIEND_ACCOUNT_REACH_LIST_COMMAND);
      return accounts.map((account) => ({
        ...account,
        checked: saved.get(`${account.serviceId}:${account.accountId}`) ?? false,
      }));
    },
  };
}

async function waitForCompletion(completions: FriendWideWhitelistPressResult[]): Promise<void> {
  for (let attempt = 0; attempt < 20 && completions.length === 0; attempt += 1) {
    await new Promise((resolve) => { setTimeout(resolve, 0); });
  }
}

describe("TASK 0261 - connect friend-wide whitelist buttons", () => {
  it("one fixture press changes all visible account ticks together and refreshes from the Hub", async () => {
    const { markup, everywhere, nowhere, ticks, root } = fixture();
    const commands: string[] = [];
    const received: { action: "everywhere" | "nowhere"; accountIds: string[] }[] = [];
    const completions: FriendWideWhitelistPressResult[] = [];

    expect(FRIEND_WHITELIST_EVERYWHERE_COMMAND).toBe("set_hub_friend_account_reach_everywhere");
    expect(FRIEND_WHITELIST_NOWHERE_COMMAND).toBe("set_hub_friend_account_reach_nowhere");
    expect(markup).toContain("Whitelist everywhere");
    expect(markup).toContain("Whitelist nowhere");
    expect(ticks.map((tick) => tick.checked)).toEqual([false, false, false]);
    expect(connectFriendWideWhitelistButtons(root, ACCOUNTS, {
      commands: fakeHub(commands, received),
      onComplete: (result) => { completions.push(result); },
    })).toBe(2);

    everywhere.press();
    await waitForCompletion(completions);

    expect(commands).toEqual([
      FRIEND_WHITELIST_EVERYWHERE_COMMAND,
      FRIEND_ACCOUNT_REACH_LIST_COMMAND,
    ]);
    expect(completions[0]?.action).toBe("everywhere");
    expect(completions[0]?.refreshed).toHaveLength(3);
    expect(received[0]).toEqual({
      action: "everywhere",
      accountIds: ["discord-main", "telegram-work", "signal-private"],
    });
    expect(ticks.map((tick) => tick.checked)).toEqual([true, true, true]);
    console.log(`TASK0261 press=everywhere visible-ticks=${ticks.length} changed-ticks=3 states=${ticks.map((tick) => tick.checked).join(",")} refreshes=${commands.filter((command) => command === FRIEND_ACCOUNT_REACH_LIST_COMMAND).length}`);

    commands.length = 0;
    completions.length = 0;
    nowhere.press();
    await waitForCompletion(completions);
    expect(commands).toEqual([
      FRIEND_WHITELIST_NOWHERE_COMMAND,
      FRIEND_ACCOUNT_REACH_LIST_COMMAND,
    ]);
    expect(received[1]).toEqual({
      action: "nowhere",
      accountIds: ["discord-main", "telegram-work", "signal-private"],
    });
    expect(ticks.map((tick) => tick.checked)).toEqual([false, false, false]);
    console.log(`TASK0261 press=nowhere visible-ticks=${ticks.length} changed-ticks=3 states=${ticks.map((tick) => tick.checked).join(",")} refreshes=${commands.filter((command) => command === FRIEND_ACCOUNT_REACH_LIST_COMMAND).length}`);
  });
});
