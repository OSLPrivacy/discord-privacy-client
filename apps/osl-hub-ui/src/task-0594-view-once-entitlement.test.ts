import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { uiProGates } from "./entitlement-gates";
import {
  oslChatsViewMarkup,
  type OslChatFriend,
  type OslChatsViewModel,
} from "./osl-chats-view";

const FREE_CREATE_SENTENCE = "Making a view-once message needs Pro.";
const FREE_OPEN_SENTENCE = "Opening a view-once message is always free.";

function friend(): OslChatFriend {
  return {
    personId: "task-0594-friend",
    nickname: "Rose",
    verified: true,
    ready: true,
    handshakeConfirmed: true,
    preview: null,
    previewVisible: true,
    unreadCount: 0,
  };
}

function account(canCreateViewOnce: boolean, busy = false): OslChatsViewModel {
  return {
    friends: [friend()],
    activePersonId: "task-0594-friend",
    messages: [],
    draft: "",
    busy,
    viewOnce: false,
    canCreateViewOnce,
  };
}

function viewOnceInput(markup: string): string {
  return markup.match(/<input id="osl-chat-view-once"[^>]*>/u)?.[0] ?? "";
}

function disabledWithoutExplanation(markups: readonly string[]): number {
  return markups.filter((markup) => {
    const input = viewOnceInput(markup);
    if (!input.includes("disabled")) return false;
    const describedBy = input.match(/aria-describedby="([^"]+)"/u)?.[1];
    return !describedBy || !markup.includes(`id="${describedBy}"`);
  }).length;
}

describe("TASK 0594 view-once entitlement explanation", () => {
  it("turns creation off for Free, explains both sides, and leaves it on for Pro", () => {
    const free = oslChatsViewMarkup(account(false));
    const pro = oslChatsViewMarkup(account(true));
    const proBusy = oslChatsViewMarkup(account(true, true));
    const freeInput = viewOnceInput(free);
    const proInput = viewOnceInput(pro);

    const freeButtonOff = Number(freeInput.includes("disabled"));
    const freeCreateSentenceCount = free.split(FREE_CREATE_SENTENCE).length - 1;
    const freeOpenSentenceCount = free.split(FREE_OPEN_SENTENCE).length - 1;
    const proButtonOn = Number(!proInput.includes("disabled"));
    const unexplainedOffCount = disabledWithoutExplanation([free, pro, proBusy]);
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    const explanationRule = styles.match(/\.osl-chat-view-once-explanation\s*\{[^}]+\}/u)?.[0] ?? "";
    const freeExplanationVisible = Number(
      free.includes('<p class="osl-chat-view-once-explanation"')
      && !/display\s*:\s*none|visibility\s*:\s*hidden|clip\s*:/u.test(explanationRule),
    );

    console.log(`TASK0594_FREE_ACCOUNT_BUTTON_OFF=${freeButtonOff}`);
    console.log(`TASK0594_FREE_CREATE_SENTENCE_COUNT=${freeCreateSentenceCount} text=${FREE_CREATE_SENTENCE}`);
    console.log(`TASK0594_FREE_OPEN_SENTENCE_COUNT=${freeOpenSentenceCount} text=${FREE_OPEN_SENTENCE}`);
    console.log(`TASK0594_FREE_EXPLANATION_VISIBLE=${freeExplanationVisible}`);
    console.log(`TASK0594_PRO_ACCOUNT_BUTTON_ON=${proButtonOn}`);
    console.log(`TASK0594_DISABLED_WITHOUT_EXPLANATION=${unexplainedOffCount}`);

    expect(freeButtonOff).toBe(1);
    expect(freeCreateSentenceCount).toBe(1);
    expect(freeOpenSentenceCount).toBe(1);
    expect(freeInput).toContain('aria-describedby="osl-chat-view-once-explanation"');
    expect(freeInput).not.toContain("checked");
    expect(freeExplanationVisible).toBe(1);
    expect(proButtonOn).toBe(1);
    expect(pro).not.toContain(FREE_CREATE_SENTENCE);
    expect(unexplainedOffCount).toBe(0);
  });

  it("records that the UI gate is backed by native enforcement", () => {
    expect(uiProGates).toContainEqual(expect.objectContaining({
      id: "view-once-message-creation",
      enforcement: "native",
    }));
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(main).toContain("canCreateViewOnce: pro");
  });
});
