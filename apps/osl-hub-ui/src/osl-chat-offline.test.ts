import { describe, expect, it } from "vitest";

import { queuedOslChatMessage } from "./osl-chat-runtime";
import { oslChatsViewMarkup } from "./osl-chats-view";

describe("T14-T13 offline OSL Chat state", () => {
  it("renders a durably queued message as queued, never delivered", () => {
    const queued = queuedOslChatMessage("queued-1", "written while offline", "Now");
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

    expect(markup).toContain('is-queued">Queued — not sent');
    expect(markup).not.toContain('is-delivered">Delivered');
  });
});
