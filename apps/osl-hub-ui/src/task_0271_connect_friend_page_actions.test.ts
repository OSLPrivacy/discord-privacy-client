import { describe, expect, it } from "vitest";
import { FRIEND_PAGE_ACTION_COMMANDS, connectFriendPageActions, type FriendPageAction } from "./friend-page-actions-connect";

const PERSON_ID = "hub-person-task-0271";

class Control {
  readonly dataset: { oslChatOpen?: string; removePerson?: string; blockPerson?: string };
  private listener: (() => void) | null = null;
  constructor(action: FriendPageAction) {
    this.dataset = action === "message" ? { oslChatOpen: PERSON_ID }
      : action === "remove" ? { removePerson: PERSON_ID } : { blockPerson: PERSON_ID };
  }
  addEventListener(_type: "click", listener: () => void): void { this.listener = listener; }
  press(): void { this.listener?.(); }
}

async function settle(results: unknown[]): Promise<void> {
  for (let attempt = 0; attempt < 20 && results.length < 3; attempt += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("TASK 0271 - connect the friend page actions", () => {
  it("returns the expected direct command result for every fixture action", async () => {
    const message = new Control("message");
    const remove = new Control("remove");
    const block = new Control("block");
    const controls = { message, remove, block };
    const results: Array<{ action: FriendPageAction; command: string; personId: string }> = [];
    const commands = Object.fromEntries((Object.keys(FRIEND_PAGE_ACTION_COMMANDS) as FriendPageAction[]).map((action) => [
      action,
      async (personId: string) => ({ action, command: FRIEND_PAGE_ACTION_COMMANDS[action], personId }),
    ])) as {
      [Action in FriendPageAction]: (personId: string) => Promise<{ action: Action; command: string; personId: string }>;
    };
    const root = { querySelectorAll(selector: string) {
      if (selector.includes("Message")) return [message];
      if (selector.includes("remove")) return [remove];
      if (selector.includes("block")) return [block];
      return [];
    } };

    expect(connectFriendPageActions(root, commands, (action, result) => results.push({ action, ...result }))).toBe(3);
    controls.message.press(); controls.remove.press(); controls.block.press();
    await settle(results);

    expect(results).toEqual([
      { action: "message", command: "activate_osl_chat_context", personId: PERSON_ID },
      { action: "remove", command: "remove_hub_friend", personId: PERSON_ID },
      { action: "block", command: "osl_block_friend_request", personId: PERSON_ID },
    ]);
    console.log(`TASK0271 commands=${results.map((result) => result.command).join(",")} results=${results.length} person=${PERSON_ID}`);
  });
});
