import { describe, expect, it, vi } from "vitest";

import {
  addFriendByNameBoxMarkup,
  bindAddFriendByNameForm,
  type AddFriendByNameFormRoot,
  type FriendNameRequestDependencies,
  type PendingFriendRequestEntry,
} from "./ui-behavior";

const escapeHtml = (value: string) => value.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

class FakeInput {
  value = "";
}

class FakeStatus {
  textContent = "";
}

class FakeList {
  innerHTML = "";
}

class FakeButton extends EventTarget {}

/**
 * Stands in for `document`: resolves the exact four selectors
 * `bindAddFriendByNameForm` queries for, backed by the real markup emitted by
 * `addFriendByNameBoxMarkup` so the ids stay in lockstep with what main.ts
 * actually renders into the Friends dialog.
 */
class FakeFriendsDialogRoot implements AddFriendByNameFormRoot {
  readonly nameInput = new FakeInput();
  readonly sendButton = new FakeButton();
  readonly statusElement = new FakeStatus();
  readonly listElement = new FakeList();

  querySelector(selector: string): unknown {
    switch (selector) {
      case "#friend-osl-name-input": return this.nameInput;
      case "#send-friend-request-by-name": return this.sendButton;
      case "#friend-name-request-status": return this.statusElement;
      case "#pending-friend-requests-list": return this.listElement;
      default: return null;
    }
  }
}

describe("task 0214: connect Send Request to the exact-name request command", () => {
  it("clicking Send Request calls the exact-name command and adds the recipient to Pending", async () => {
    const markup = addFriendByNameBoxMarkup();
    expect(markup).toContain('id="friend-osl-name-input"');
    expect(markup).toContain('id="send-friend-request-by-name"');
    expect(markup).toContain('id="pending-friend-requests-list"');

    const root = new FakeFriendsDialogRoot();
    const pending: PendingFriendRequestEntry[] = [];
    const createRequest = vi.fn(async (name: string) => ({ requestId: `req-${name}`, recipientName: name }));
    const dependencies: FriendNameRequestDependencies = { createRequest };

    bindAddFriendByNameForm(root, pending, dependencies, escapeHtml);

    root.nameInput.value = "maple_0213";
    root.sendButton.dispatchEvent(new Event("click"));
    await vi.waitFor(() => expect(pending.length).toBe(1));

    expect(createRequest).toHaveBeenCalledExactlyOnceWith("maple_0213");
    expect(pending).toEqual([{ requestId: "req-maple_0213", recipientName: "maple_0213" }]);
    expect(root.listElement.innerHTML).toContain("Pending: maple_0213");
    expect(root.statusElement.textContent).toBe("");
    expect(root.nameInput.value).toBe("");

    // eslint-disable-next-line no-console
    console.log(`TASK_0214_PENDING_REFRESH pending_count=${pending.length} list_html=${root.listElement.innerHTML}`);
  });

  it("an unknown name is refused and Pending stays empty", async () => {
    const root = new FakeFriendsDialogRoot();
    const pending: PendingFriendRequestEntry[] = [];
    const dependencies: FriendNameRequestDependencies = { createRequest: async () => null };

    bindAddFriendByNameForm(root, pending, dependencies, escapeHtml);

    root.nameInput.value = "nobody_here";
    root.sendButton.dispatchEvent(new Event("click"));
    await vi.waitFor(() => expect(root.statusElement.textContent.length).toBeGreaterThan(0));

    expect(pending).toEqual([]);
    expect(root.statusElement.textContent).toContain("nobody_here");
    expect(root.listElement.innerHTML).toBe("");
  });
});
