import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

import { queuedOslChatMessage } from "./osl-chat-runtime";
import { oslChatsViewMarkup } from "./osl-chats-view";
import { offlineCapabilityStatus } from "./offline-capability-status";

describe("T14-T13 offline OSL Chat state", () => {
  it("renders a durably queued message as queued, never delivered", () => {
    const queued = queuedOslChatMessage("queued-1", "written while offline", "Now");
    expect(queued.state).toBe("queued");
    const markup = oslChatsViewMarkup({
      friends: [{
        personId: "friend-1", nickname: "Friend", verified: true, ready: true,
        preview: null, previewVisible: true, unreadCount: 0,
      }],
      activePersonId: "friend-1",
      messages: [queued],
      draft: "",
      busy: false,
    });

    expect(markup).toContain('is-queued">Not sent');
    expect(markup).not.toContain("Queued");
    expect(markup).not.toContain('is-sent">Sent');
    expect(markup).not.toContain('is-delivered">Delivered');
  });

  it("does not promise a production offline-send queue while ordinary OSL Chat posts are non-idempotent", () => {
    expect(offlineCapabilityStatus("sendMessage", "offline").detail).toMatch(/not queued/i);

    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const sendStart = main.indexOf("async function sendOslChat(");
    const sendEnd = main.indexOf("function resetOslChatUiState(", sendStart);
    const sendBody = main.slice(sendStart, sendEnd);

    expect(sendStart).toBeGreaterThanOrEqual(0);
    expect(sendEnd).toBeGreaterThan(sendStart);
    expect(sendBody.indexOf('refuseOfflineCapability("sendMessage")')).toBeLessThan(sendBody.indexOf("prepareOslChatText("));
    expect(sendBody).not.toContain("queuedOslChatMessage(");
    expect(main).not.toMatch(/import\s*\{[^}]*queuedOslChatMessage/u);
  });
});
