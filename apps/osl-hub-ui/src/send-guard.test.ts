import { describe, expect, it } from "vitest";
import { TrustedSendGuard, type TrustedComposerSnapshot } from "./send-guard";

const snapshot: TrustedComposerSnapshot = {
  serviceId: "discord",
  accountId: "account-1",
  conversationId: "conversation-1",
  composerId: "composer-1",
  hostGeneration: 4,
  focused: true,
  exactComposerVerified: true,
};

describe("TrustedSendGuard", () => {
  it("copies without ever producing a submit decision", () => {
    const guard = new TrustedSendGuard();
    expect(guard.accept("clipboard", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, snapshot)).toEqual({ action: "copy" });
  });

  it("requires two distinct trusted enters and an unchanged exact snapshot", () => {
    const guard = new TrustedSendGuard();
    expect(guard.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, snapshot).action).toBe("place");
    expect(guard.accept("double", { eventId: "two", isTrusted: true, occurredAtMs: 100 }, snapshot)).toEqual({ action: "submit" });
  });

  it("cancels on repeated, expired, changed, or unverified input", () => {
    const repeated = new TrustedSendGuard();
    repeated.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, snapshot);
    expect(repeated.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 20 }, snapshot)).toEqual({ action: "reject", reason: "repeated" });

    const expired = new TrustedSendGuard();
    expired.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, snapshot);
    expect(expired.accept("double", { eventId: "two", isTrusted: true, occurredAtMs: 2_511 }, snapshot)).toEqual({ action: "reject", reason: "expired" });

    const changed = new TrustedSendGuard();
    changed.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, snapshot);
    expect(changed.accept("double", { eventId: "two", isTrusted: true, occurredAtMs: 20 }, { ...snapshot, conversationId: "other" })).toEqual({ action: "reject", reason: "changed" });

    expect(new TrustedSendGuard().accept("single", { eventId: "one", isTrusted: false, occurredAtMs: 10 }, snapshot)).toEqual({ action: "reject", reason: "untrusted" });
    expect(new TrustedSendGuard().accept("single", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, { ...snapshot, exactComposerVerified: false })).toEqual({ action: "reject", reason: "unverified" });
  });
});
