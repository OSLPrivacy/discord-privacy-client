import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import {
  bindFriendRemovalControls,
  FRIEND_REMOVAL_SELECTOR,
  friendRemovalButtonMarkup,
  friendTrustAction,
  removeHubFriend,
  shouldClearRemovedFriendChat,
  type FriendRemovalControl,
  type FriendRemovalRoot,
} from "./ui-behavior";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

class RenderedFriendRemovalButton extends EventTarget implements FriendRemovalControl {
  readonly dataset: { readonly removePerson?: string };

  constructor(markup: string) {
    super();
    const match = markup.match(/\sdata-remove-person="([^"]*)"/);
    this.dataset = { removePerson: match?.[1] };
  }
}

describe("friend trust UI", () => {
  it("requires a peer-entered verification code and cannot round-trip the displayed code", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    const dialog = functionSource("ownedConfirmationMarkup", "serviceContent");
    const binding = functionSource("bindOwnedConfirmation", "resetLocalProtectedSheet");
    expect(dialog).toContain('id="friend-verification-input"');
    expect(dialog).toContain('autocomplete="off" spellcheck="false" inputmode="numeric"');
    expect(dialog).toContain("person?.safetyNumber");
    expect(dialog).toContain("over a channel that is not this app");
    expect(dialog).toContain("lets OSL encrypt to the key it holds for this friend");
    expect(people).not.toContain("data-safety-number");
    expect(binding).toContain("input.value.length === 0");
    expect(binding).toContain("ownedConfirmationBusy");
  });

  it("renders inert Verified only for verified friends without a pending key change", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    expect(friendTrustAction(true, false)).toBe("verified");
    expect(friendTrustAction(true, true)).toBe("verify");
    expect(friendTrustAction(false, false)).toBe("verify");
    expect(friendTrustAction(false, true)).toBe("verify");
    expect(people).toContain('trustAction === "verified"');
    expect(people).toContain("Re-verify key");
    expect(people).toContain("data-verify-person");
  });

  it("passes the raw typed value and keeps a named refusal visible without replacing the input", () => {
    const execute = functionSource("executeOwnedConfirmation", "createAdditionalIdentity");
    expect(execute).toContain("verificationInput?.value");
    expect(execute).toContain("verifyHubPerson(request.personId, typedVerificationCode)");
    expect(execute).not.toContain(".trim()");
    expect(execute).not.toContain("request.verificationCode");
    expect(execute).toContain("Verification refused:");
    expect(execute).toContain("status.textContent = message");
    expect(execute).not.toContain("render();");
  });

  it("routes friend removal through the owned confirmation before calling the adapter", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    const request = functionSource("requestFriendRemoval", "allowPersonHere");
    const execute = functionSource("executeOwnedConfirmation", "createAdditionalIdentity");
    const dialog = functionSource("ownedConfirmationMarkup", "serviceContent");
    expect(people).toContain('mode === "manage"');
    expect(people).toContain("friendRemovalButtonMarkup(person.personId, escapeHtml)");
    expect(request).toContain('ownedConfirmation = { kind: "removeFriend", personId }');
    expect(request).not.toContain("removeHubFriend(");
    expect(execute).toContain("removeHubFriend(request.personId, { isTauriRuntime, invoke, recordBackendFailure })");
    expect(dialog).toContain("withdraws every conversation approval they hold");
    expect(dialog).toContain("This cannot be undone.");
  });

  it("dispatches the rendered remove-button click through the production binding", () => {
    const markup = friendRemovalButtonMarkup("hub-person-abc", (value) => value);
    const button = new RenderedFriendRemovalButton(markup);
    const requestFriendRemoval = vi.fn();
    const selectors: string[] = [];
    const root: FriendRemovalRoot = {
      querySelectorAll: (selector) => {
        selectors.push(selector);
        return [button];
      },
    };

    bindFriendRemovalControls(root, requestFriendRemoval);
    button.dispatchEvent(new Event("click"));

    expect(markup).toContain('data-remove-person="hub-person-abc"');
    expect(selectors).toEqual([FRIEND_REMOVAL_SELECTOR]);
    expect(requestFriendRemoval).toHaveBeenCalledOnce();
    expect(requestFriendRemoval).toHaveBeenCalledWith("hub-person-abc");
  });

  it("wires the rendered remove controls to the behavioural binding", () => {
    const workspace = functionSource("bindWorkspace", "openHomeAppFromLauncher");
    expect(workspace).toContain("bindFriendRemovalControls(");
    expect(workspace).toContain("document.querySelectorAll<HTMLButtonElement>(selector)");
    expect(workspace).toContain("requestFriendRemoval,");
  });

  it("clears an active chat only when that friend was removed", () => {
    const execute = functionSource("executeOwnedConfirmation", "createAdditionalIdentity");
    expect(shouldClearRemovedFriendChat("friend-a", "friend-a")).toBe(true);
    expect(shouldClearRemovedFriendChat("friend-a", "friend-b")).toBe(false);
    expect(shouldClearRemovedFriendChat(null, "friend-a")).toBe(false);
    expect(execute).toContain("shouldClearRemovedFriendChat(activeOslChatPersonId, request.personId)");
    expect(execute).toContain("resetOslChatUiState(false)");
    expect(execute).toContain("hubPeople = await listHubPeople()");
    expect(execute).toContain("closeOwnedConfirmation()");
    expect(execute).toContain("Friend removed · keys and conversation approvals withdrawn");
  });

  it("invokes friend removal with the exact person id and fails closed", async () => {
    const invoke = vi.fn(async () => undefined);
    const recordBackendFailure = vi.fn();
    const dependencies = { isTauriRuntime: () => true, invoke, recordBackendFailure };
    await expect(removeHubFriend("hub-person-abc", dependencies)).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("remove_hub_friend", { personId: "hub-person-abc" });

    invoke.mockRejectedValueOnce(new Error("command unavailable"));
    await expect(removeHubFriend("hub-person-abc", dependencies)).resolves.toBe(false);
    expect(recordBackendFailure).toHaveBeenCalledWith("remove_hub_friend", expect.any(Error));

    invoke.mockClear();
    await expect(removeHubFriend("", dependencies)).resolves.toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });
});
