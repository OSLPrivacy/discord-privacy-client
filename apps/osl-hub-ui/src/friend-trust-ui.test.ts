import { describe, expect, it, vi } from "vitest";
import {
  bindFriendRemovalControls,
  FRIEND_REMOVAL_SELECTOR,
  friendRemovalButtonMarkup,
  friendTrustAction,
  friendVerificationCopy,
  ownedConfirmationSubmitDisabled,
  removeHubFriend,
  shouldClearRemovedFriendChat,
  verificationSubmission,
  EMPTY_VERIFICATION_CODE_REFUSAL,
  type FriendRemovalControl,
  type FriendRemovalRoot,
} from "./ui-behavior";

class RenderedFriendRemovalButton extends EventTarget implements FriendRemovalControl {
  readonly dataset: { readonly removePerson?: string };

  constructor(markup: string) {
    super();
    const match = markup.match(/\sdata-remove-person="([^"]*)"/);
    this.dataset = { removePerson: match?.[1] };
  }
}

describe("friend trust UI", () => {
  it("tells the operator to compare with the friend's screen, never to relay their own code", () => {
    const copy = friendVerificationCopy("Rosalind", "12345 67890 12345 67890 12345 67890");
    const spoken = `${copy.heading} ${copy.instruction}`;

    // The number is shared, so the instruction must send the operator to the
    // other device's screen. It is the only place a value OSL can accept lives.
    expect(copy.heading).toContain("Rosalind");
    expect(copy.heading).toMatch(/same code/i);
    expect(copy.instruction).toMatch(/on their screen/i);
    expect(copy.instruction).toMatch(/not this app/i);
    expect(copy.code).toBe("12345 67890 12345 67890 12345 67890");

    // The ceremony that never worked: read your own code out, type back what
    // you hear. Following that literally makes verification fail.
    expect(spoken).not.toMatch(/read (it|this|the code) aloud/i);
    expect(spoken).not.toMatch(/read back/i);

    // The upgrade is a trust regression and has to be said out loud.
    expect(copy.invalidationNotice).toMatch(/cleared/i);
    expect(copy.invalidationNotice).toMatch(/verified again/i);
    expect(copy.consequence).toMatch(/does not turn on decryption/i);

    // No number yet is stated, never faked with placeholder digits.
    expect(friendVerificationCopy(null, null).code).toBe("Unavailable");
    expect(friendVerificationCopy(null, "").code).toBe("Unavailable");
    expect(friendVerificationCopy(null, null).heading).toContain("this friend");
  });

  it("accepts a code that arrived without an input event, and still refuses a blank one", () => {
    // A paste that fires no `input`, or any programmatic fill. The button used
    // to render disabled for the whole verify dialog and was re-enabled only by
    // an observed `input` event, so such a value left a dead control on the one
    // screen a first-time user has to get through.
    const field = { value: "" };
    expect(ownedConfirmationSubmitDisabled(false)).toBe(false);

    field.value = "12345 67890 12345 67890 12345 67890";
    expect(verificationSubmission(field.value)).toEqual({ code: field.value });

    // The value reaches the backend exactly as it arrived — grouping and
    // separators are normalised by the constant-time comparison in Rust.
    expect(verificationSubmission("  123-456 789\t012 345 678 901 234 567 890 "))
      .toEqual({ code: "  123-456 789\t012 345 678 901 234 567 890 " });

    // Not a control that always says yes: blank is refused, and refused
    // visibly rather than by making the button unreachable.
    expect(verificationSubmission("")).toEqual({ refusal: EMPTY_VERIFICATION_CODE_REFUSAL });
    expect(verificationSubmission("   \n\t ")).toEqual({ refusal: EMPTY_VERIFICATION_CODE_REFUSAL });
    expect(EMPTY_VERIFICATION_CODE_REFUSAL.length).toBeGreaterThan(0);

    // A submit already in flight is the one thing that takes the button away.
    expect(ownedConfirmationSubmitDisabled(true)).toBe(true);
  });

  it("offers the verified action only for a verified friend without a pending key change", () => {
    expect(friendTrustAction(true, false)).toBe("verified");
    expect(friendTrustAction(true, true)).toBe("verify");
    expect(friendTrustAction(false, false)).toBe("verify");
    expect(friendTrustAction(false, true)).toBe("verify");
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

  it("clears an active chat only when that friend was removed", () => {
    expect(shouldClearRemovedFriendChat("friend-a", "friend-a")).toBe(true);
    expect(shouldClearRemovedFriendChat("friend-a", "friend-b")).toBe(false);
    expect(shouldClearRemovedFriendChat(null, "friend-a")).toBe(false);
    expect(shouldClearRemovedFriendChat("friend-a", "")).toBe(false);
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
