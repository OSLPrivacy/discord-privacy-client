import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  BLOCKED_TAB_BUTTON_SELECTOR,
  connectBlockedTabRows,
  OslBlockedTabConnector,
  type BlockedTabButton,
} from "./blocked-tab-connect";
import { listHubBlockedPeople, type HubBlockedPerson } from "./adapters";

/**
 * A stand-in for gate 0280's backend (crates/ipc/src/commands.rs):
 *   list_hub_blocked_people -> every row in `blocked_people.json`;
 *   unblock_hub_person      -> removes exactly the named row and reports the
 *                              new count, following cmd_osl_unblock_person.
 */
class FakeBlockedPeopleBackend {
  records: HubBlockedPerson[] = [
    { peerDiscordId: "900000000000002800", state: "Blocked", blockedAtUnixSeconds: 1000 },
    { peerDiscordId: "900000000000002801", state: "Blocked", blockedAtUnixSeconds: 2000 },
  ];
  unblockCalls: unknown[] = [];

  invoke = async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
    if (command === "list_hub_blocked_people") {
      return this.records.map((record) => ({ ...record }));
    }
    if (command === "unblock_hub_person") {
      this.unblockCalls.push(args);
      const { peerDiscordId } = args as { peerDiscordId: string };
      const before = this.records.length;
      this.records = this.records.filter((record) => record.peerDiscordId !== peerDiscordId);
      const removedFromBlocked = this.records.length !== before;
      return {
        personId: peerDiscordId,
        removedFromBlocked,
        blockedCount: this.records.length,
        friendshipState: "none",
        allowedPlaces: 0,
      };
    }
    throw new Error(`unexpected command ${command}`);
  };
}

class RenderedButton implements BlockedTabButton {
  private readonly attributes: Record<string, string>;
  private readonly listeners: ((event: unknown) => unknown)[] = [];

  constructor(attributes: Record<string, string>) {
    this.attributes = attributes;
  }

  getAttribute(name: string): string | null {
    return this.attributes[name] ?? null;
  }

  addEventListener(type: string, listener: (event: unknown) => void): void {
    if (type === "click") this.listeners.push(listener);
  }

  async click(): Promise<void> {
    await Promise.all(this.listeners.map((listener) => listener({ type: "click" })));
  }
}

/** Every button in the markup this task draws, read back out of that markup. */
function renderedButtons(markup: string): RenderedButton[] {
  const buttons: RenderedButton[] = [];
  const tags = markup.matchAll(/<button\b([^>]*)>/gu);
  for (const tag of tags) {
    const source = tag[1] ?? "";
    const attributes: Record<string, string> = {};
    const pattern = /([\w-]+)="([^"]*)"/gu;
    let attribute = pattern.exec(source);
    while (attribute !== null) {
      attributes[attribute[1]] = attribute[2];
      attribute = pattern.exec(source);
    }
    buttons.push(new RenderedButton(attributes));
  }
  return buttons;
}

function buttonRoot(buttons: readonly RenderedButton[]) {
  return {
    querySelectorAll(selector: string): RenderedButton[] {
      expect(selector).toBe(BLOCKED_TAB_BUTTON_SELECTOR);
      return [...buttons];
    },
  };
}

/** A direct query, straight to the command -- not what the screen believes. */
async function blockedCount(): Promise<number> {
  const rows = await listHubBlockedPeople();
  expect(rows).not.toBeNull();
  return (rows ?? []).length;
}

function findButton(buttons: readonly RenderedButton[], action: string, personId: string): RenderedButton {
  const found = buttons.find(
    (button) => button.getAttribute("data-blocked-action") === action && button.getAttribute("data-blocked-person") === personId,
  );
  if (!found) throw new Error(`no ${action} button for ${personId}`);
  return found;
}

let backend: FakeBlockedPeopleBackend;
const TARGET_PERSON = "900000000000002800";
const OTHER_PERSON = "900000000000002801";

beforeEach(() => {
  backend = new FakeBlockedPeopleBackend();
  mocks.invoke.mockImplementation(backend.invoke);
  mocks.isTauriRuntime.mockReturnValue(true);
});

describe("task 0281: connect Unblock to the Blocked tab", () => {
  it("connects one Unblock control per drawn row", async () => {
    const connector = new OslBlockedTabConnector();
    await connector.refresh();
    const buttons = renderedButtons(connector.render());
    const connected = connectBlockedTabRows(buttonRoot(buttons), connector);
    expect(connected).toBe(2);
    expect(buttons.filter((b) => b.getAttribute("data-blocked-action") === "unblock")).toHaveLength(2);
  });

  it("starts from the backend's own count of 2", async () => {
    expect(await blockedCount()).toBe(2);
  });

  it("pressing Unblock without confirming leaves the Blocked count unchanged", async () => {
    const connector = new OslBlockedTabConnector();
    await connector.refresh();
    const buttons = renderedButtons(connector.render());
    connectBlockedTabRows(buttonRoot(buttons), connector);

    await findButton(buttons, "unblock", TARGET_PERSON).click();

    expect(backend.unblockCalls).toHaveLength(0);
    expect(await blockedCount()).toBe(2);
    expect(connector.rows).toHaveLength(2);
    expect(connector.render()).toContain(`data-blocked-person="${TARGET_PERSON}"`);
  });

  it("confirming takes the Blocked count from 2 to 1 and the row disappears", async () => {
    const connector = new OslBlockedTabConnector();
    await connector.refresh();
    let buttons = renderedButtons(connector.render());
    connectBlockedTabRows(buttonRoot(buttons), connector);

    await findButton(buttons, "unblock", TARGET_PERSON).click();
    // Second render shows the confirm control for the same row.
    buttons = renderedButtons(connector.render());
    connectBlockedTabRows(buttonRoot(buttons), connector);

    const before = await blockedCount();
    await findButton(buttons, "confirm-unblock", TARGET_PERSON).click();
    await vi.waitFor(() => expect(backend.unblockCalls).toHaveLength(1));
    const after = await blockedCount();

    expect(before).toBe(2);
    expect(after).toBe(1);
    expect(backend.unblockCalls).toEqual([{ peerDiscordId: TARGET_PERSON }]);
    expect(connector.rows).toHaveLength(1);
    expect(connector.rows.some((row) => row.peerDiscordId === TARGET_PERSON)).toBe(false);
    expect(connector.render()).not.toContain(`data-blocked-person="${TARGET_PERSON}"`);
    expect(connector.render()).toContain(`data-blocked-person="${OTHER_PERSON}"`);
  });

  it("never touches a neighbouring row's identifier", async () => {
    const connector = new OslBlockedTabConnector();
    await connector.refresh();
    const buttons = renderedButtons(connector.render());
    connectBlockedTabRows(buttonRoot(buttons), connector);

    await findButton(buttons, "unblock", TARGET_PERSON).click();
    connector.confirmingPersonId = TARGET_PERSON;
    await connector.confirmUnblock(TARGET_PERSON);

    expect(backend.unblockCalls).toEqual([{ peerDiscordId: TARGET_PERSON }]);
    expect(JSON.stringify(backend.unblockCalls)).not.toContain(OTHER_PERSON);
  });

  it("Cancel clears the pending confirmation without sending a command", async () => {
    const connector = new OslBlockedTabConnector();
    await connector.refresh();
    let buttons = renderedButtons(connector.render());
    connectBlockedTabRows(buttonRoot(buttons), connector);

    await findButton(buttons, "unblock", TARGET_PERSON).click();
    buttons = renderedButtons(connector.render());
    connectBlockedTabRows(buttonRoot(buttons), connector);
    await findButton(buttons, "cancel-unblock", TARGET_PERSON).click();

    expect(connector.confirmingPersonId).toBeNull();
    expect(connector.render()).toContain(`data-blocked-action="unblock" data-blocked-person="${TARGET_PERSON}"`);
    expect(backend.unblockCalls).toHaveLength(0);
  });

  it("confirming a different row than was asked about sends nothing", async () => {
    const connector = new OslBlockedTabConnector();
    await connector.refresh();
    connector.requestUnblock(TARGET_PERSON);
    const result = await connector.confirmUnblock(OTHER_PERSON);

    expect(result).toBeNull();
    expect(backend.unblockCalls).toHaveLength(0);
    expect(await blockedCount()).toBe(2);
  });
});
