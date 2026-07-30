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

  it("returns an immutable trusted-composer snapshot for a single-send handoff", () => {
    const mutable = { ...snapshot };
    const decision = new TrustedSendGuard().accept("single", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, mutable);
    if (decision.action !== "submit") throw new Error("expected submit");

    expect(decision.snapshot).toEqual(snapshot);
    expect(decision.snapshot).not.toBe(mutable);
    expect(Object.isFrozen(decision.snapshot)).toBe(true);

    mutable.conversationId = "changed-after-handoff";
    expect(decision.snapshot.conversationId).toBe("conversation-1");
    expect(() => {
      (decision.snapshot as TrustedComposerSnapshot).conversationId = "mutated";
    }).toThrow(TypeError);
    expect(decision.snapshot.conversationId).toBe("conversation-1");
  });

  it("requires two distinct trusted enters and an unchanged exact snapshot", () => {
    const guard = new TrustedSendGuard();
    const placement = guard.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, snapshot);
    if (placement.action !== "place") throw new Error("expected place");

    expect(placement.expiresAtMs).toBe(2_510);
    expect(placement.snapshot).toEqual(snapshot);
    expect(Object.isFrozen(placement.snapshot)).toBe(true);

    const submit = guard.accept("double", { eventId: "two", isTrusted: true, occurredAtMs: 100 }, snapshot);
    expect(submit).toEqual({ action: "submit", snapshot: placement.snapshot });
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

  it("keeps the armed handoff detached from later caller mutation", () => {
    const guard = new TrustedSendGuard();
    const mutable = { ...snapshot };
    const placement = guard.accept("double", { eventId: "one", isTrusted: true, occurredAtMs: 10 }, mutable);
    if (placement.action !== "place") throw new Error("expected place");

    mutable.conversationId = "changed-after-arm";

    expect(placement.snapshot.conversationId).toBe("conversation-1");
    expect(guard.accept("double", { eventId: "two", isTrusted: true, occurredAtMs: 100 }, mutable)).toEqual({ action: "reject", reason: "changed" });
  });
});
