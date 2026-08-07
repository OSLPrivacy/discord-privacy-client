import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { accountReachListMarkup, type AccountReachChoice } from "./account-reach-list";
import {
  accountReachStates,
  connectAccountReachList,
  refreshAccountReachList,
  type AccountReachBox,
  type AccountReachToggle,
} from "./account-reach-connect";
import { listHubFriendAccountReachChoices, type FriendAccountReachChoice } from "./adapters";

/**
 * TASK 0245 fixture: the four named owned accounts TASK 0244 drew, now each
 * carrying the service its reach choice is keyed on.
 */
const PERSON_ID = "person-0245-vera";
const FOUR_OWNED_ACCOUNTS: AccountReachChoice[] = [
  { serviceId: "gmail", accountId: "gmail-primary", label: "Gmail (primary)", checked: true },
  { serviceId: "discord", accountId: "discord-alt", label: "Discord (alt)", checked: false },
  { serviceId: "signal", accountId: "signal-personal", label: "Signal (personal)", checked: true },
  { serviceId: "telegram", accountId: "telegram-work", label: "Telegram (work)", checked: false },
];

const COMPONENT = /^[A-Za-z0-9_-]{1,128}$/u;

/**
 * A stand-in for the backend the two commands reach, following what
 * apps/osl-hub/src/security.rs actually does:
 *   set_friend_account_reach_choice  -> validates the person and the two
 *      identifiers, then inserts one record under `${serviceId}:${accountId}`
 *      in that person's map and returns it;
 *   list_friend_account_reach_choices -> returns that person's records, in the
 *      BTreeMap's key order.
 * Nothing else in the store may move when one key is written -- which is the
 * property this test exists to check.
 */
class FakeAccountReachBackend {
  readonly records = new Map<string, FriendAccountReachChoice>();
  readonly setCalls: unknown[] = [];
  readonly listCalls: unknown[] = [];
  refuse: string | null = null;

  seed(personId: string, accounts: readonly AccountReachChoice[]): void {
    for (const account of accounts) {
      this.records.set(`${personId} ${account.serviceId}:${account.accountId}`, {
        personId,
        serviceId: account.serviceId,
        accountId: account.accountId,
        broadened: account.checked,
      });
    }
  }

  invoke = async (command: string, args: Record<string, unknown>): Promise<unknown> => {
    if (command === "set_hub_friend_account_reach_choice") {
      this.setCalls.push(args);
      const { personId, serviceId, accountId, broadened } = args as {
        personId: string; serviceId: string; accountId: string; broadened: boolean;
      };
      if (this.refuse === accountId) throw new Error("OSL friend account reach state is unavailable");
      if (!personId) throw new Error("OSL person identifier is invalid");
      if (!COMPONENT.test(serviceId)) throw new Error("OSL service identifier is invalid");
      if (!COMPONENT.test(accountId)) throw new Error("OSL account identifier is invalid");
      const record: FriendAccountReachChoice = { personId, serviceId, accountId, broadened };
      this.records.set(`${personId} ${serviceId}:${accountId}`, record);
      return { ...record };
    }
    if (command === "list_hub_friend_account_reach_choices") {
      this.listCalls.push(args);
      const { personId } = args as { personId: string };
      return [...this.records.entries()]
        .filter(([key]) => key.startsWith(`${personId} `))
        .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
        .map(([, record]) => ({ ...record }));
    }
    throw new Error(`unexpected command ${command}`);
  };
}

/** One drawn tick box, driven the way a person drives it: tick, then change. */
class RenderedBox implements AccountReachBox {
  checked: boolean;
  private readonly attributes: Record<string, string>;
  private readonly listeners: ((event: unknown) => void)[] = [];

  constructor(attributes: Record<string, string>, checked: boolean) {
    this.attributes = attributes;
    this.checked = checked;
  }

  getAttribute(name: string): string | null {
    return this.attributes[name] ?? null;
  }

  addEventListener(type: string, listener: (event: unknown) => void): void {
    if (type === "change") this.listeners.push(listener);
  }

  tick(next: boolean): void {
    this.checked = next;
    for (const listener of this.listeners) listener({ type: "change" });
  }
}

/** Every tick box in the markup TASK 0244 draws, read back out of that markup. */
function renderedBoxes(markup: string): RenderedBox[] {
  const boxes: RenderedBox[] = [];
  const inputs = markup.matchAll(/<input\b([^>]*)\/>/gu);
  for (const input of inputs) {
    const source = input[1] ?? "";
    const attributes: Record<string, string> = {};
    const pattern = /([\w-]+)(?:="([^"]*)")?/gu;
    let attribute = pattern.exec(source);
    while (attribute !== null) {
      attributes[attribute[1]] = attribute[2] ?? "";
      attribute = pattern.exec(source);
    }
    boxes.push(new RenderedBox(attributes, Object.hasOwn(attributes, "checked")));
  }
  return boxes;
}

function boxRoot(boxes: readonly RenderedBox[]) {
  return {
    querySelectorAll(selector: string): RenderedBox[] {
      expect(selector).toBe("[data-account-reach]");
      return [...boxes];
    },
  };
}

/** A direct query, straight to the command -- not what the screen believes. */
async function directQuery(): Promise<Record<string, boolean>> {
  const choices = await listHubFriendAccountReachChoices(PERSON_ID);
  expect(choices).not.toBeNull();
  return accountReachStates(FOUR_OWNED_ACCOUNTS, choices);
}

function differences(before: Record<string, boolean>, after: Record<string, boolean>): string[] {
  return Object.keys(before).filter((accountId) => before[accountId] !== after[accountId]);
}

let backend: FakeAccountReachBackend;

/** Draw the list, connect it, and hand back the boxes plus the toggles seen. */
function connectedList(): { boxes: RenderedBox[]; toggles: AccountReachToggle[]; connected: number } {
  const boxes = renderedBoxes(accountReachListMarkup(FOUR_OWNED_ACCOUNTS));
  const toggles: AccountReachToggle[] = [];
  const connected = connectAccountReachList(boxRoot(boxes), PERSON_ID, {
    onToggle: (toggle) => { toggles.push(toggle); },
  });
  return { boxes, toggles, connected };
}

async function settle(): Promise<void> {
  await new Promise((resolve) => { setTimeout(resolve, 0); });
}

beforeEach(() => {
  backend = new FakeAccountReachBackend();
  backend.seed(PERSON_ID, FOUR_OWNED_ACCOUNTS);
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(backend.invoke);
  mocks.isTauriRuntime.mockReturnValue(true);
});

describe("account reach tick boxes connected to the per-friend commands", () => {
  it("connects one tick box per drawn account", () => {
    const { boxes, connected } = connectedList();
    expect(boxes).toHaveLength(4);
    expect(connected).toBe(4);
  });

  it("starts from the four fixture rows the backend itself reports", async () => {
    expect(await directQuery()).toEqual({
      "gmail-primary": true,
      "discord-alt": false,
      "signal-personal": true,
      "telegram-work": false,
    });
  });

  it("changes only the toggled account in a direct query when a row is ticked on", async () => {
    const before = await directQuery();
    const { boxes, toggles } = connectedList();

    boxes[1].tick(true); // discord-alt: off -> on
    await settle();

    // The finish line first: a direct query, and only the toggled account moved.
    const after = await directQuery();
    expect(differences(before, after)).toEqual(["discord-alt"]);
    expect(after["discord-alt"]).toBe(true);
    expect(after).toEqual({ ...before, "discord-alt": true });

    expect(backend.setCalls).toEqual([{
      personId: PERSON_ID,
      serviceId: "discord",
      accountId: "discord-alt",
      broadened: true,
    }]);
    expect(toggles).toEqual([{
      serviceId: "discord",
      accountId: "discord-alt",
      broadened: true,
      saved: { personId: PERSON_ID, serviceId: "discord", accountId: "discord-alt", broadened: true },
    }]);
  });

  it("changes only the toggled account in a direct query when a row is ticked off", async () => {
    const before = await directQuery();
    const { boxes } = connectedList();

    boxes[2].tick(false); // signal-personal: on -> off
    await settle();

    const after = await directQuery();
    expect(differences(before, after)).toEqual(["signal-personal"]);

    expect(backend.setCalls).toEqual([{
      personId: PERSON_ID,
      serviceId: "signal",
      accountId: "signal-personal",
      broadened: false,
    }]);
    expect(after["signal-personal"]).toBe(false);
    expect(after).toEqual({ ...before, "signal-personal": false });
  });

  it("never sends a neighbouring account's identifiers", async () => {
    const { boxes } = connectedList();
    boxes[3].tick(true); // telegram-work
    await settle();

    const sent = JSON.stringify(backend.setCalls);
    for (const account of ["gmail-primary", "discord-alt", "signal-personal"]) {
      expect(sent).not.toContain(account);
    }
    expect(sent).toContain("telegram-work");
    expect((await directQuery())["telegram-work"]).toBe(true);
  });

  it("puts a refused tick back and leaves every account where it was", async () => {
    const before = await directQuery();
    backend.refuse = "gmail-primary";
    const { boxes, toggles } = connectedList();

    boxes[0].tick(false); // gmail-primary: on -> off, refused
    await settle();

    expect(toggles).toEqual([{
      serviceId: "gmail",
      accountId: "gmail-primary",
      broadened: false,
      saved: null,
      revertedTo: true,
    }]);
    expect(boxes[0].checked).toBe(true);
    expect(differences(before, await directQuery())).toEqual([]);
  });

  it("re-states the drawn list from the backend's own answer", async () => {
    const { boxes } = connectedList();
    boxes[1].tick(true);
    await settle();

    const refreshed = await refreshAccountReachList(PERSON_ID, FOUR_OWNED_ACCOUNTS);
    expect(refreshed?.map((account) => [account.accountId, account.checked])).toEqual([
      ["gmail-primary", true],
      ["discord-alt", true],
      ["signal-personal", true],
      ["telegram-work", false],
    ]);
  });

  it("reads an account the backend never recorded as not reached", () => {
    expect(accountReachStates(FOUR_OWNED_ACCOUNTS, [
      { personId: PERSON_ID, serviceId: "gmail", accountId: "gmail-primary", broadened: true },
    ])).toEqual({
      "gmail-primary": true,
      "discord-alt": false,
      "signal-personal": false,
      "telegram-work": false,
    });
  });

  it("does not re-draw the list as all-off when the query is refused", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("OSL is locked"));
    expect(await refreshAccountReachList(PERSON_ID, FOUR_OWNED_ACCOUNTS)).toBeNull();
  });
});
