import type { SendMode } from "./state";

export interface TrustedComposerSnapshot {
  serviceId: string;
  accountId: string;
  conversationId: string;
  composerId: string;
  hostGeneration: number;
  focused: boolean;
  exactComposerVerified: boolean;
}

export type ImmutableTrustedComposerSnapshot = Readonly<TrustedComposerSnapshot>;

export interface TrustedEnter {
  eventId: string;
  isTrusted: boolean;
  occurredAtMs: number;
}

export type GuardedSendDecision =
  | { action: "copy" }
  | { action: "place"; expiresAtMs: number; snapshot: ImmutableTrustedComposerSnapshot }
  | { action: "submit"; snapshot: ImmutableTrustedComposerSnapshot }
  | { action: "reject"; reason: "untrusted" | "unverified" | "expired" | "changed" | "repeated" };

interface ArmedPlacement {
  snapshot: ImmutableTrustedComposerSnapshot;
  eventId: string;
  expiresAtMs: number;
}

const DOUBLE_ENTER_WINDOW_MS = 2_500;

function sameSnapshot(left: TrustedComposerSnapshot, right: TrustedComposerSnapshot): boolean {
  return left.serviceId === right.serviceId
    && left.accountId === right.accountId
    && left.conversationId === right.conversationId
    && left.composerId === right.composerId
    && left.hostGeneration === right.hostGeneration;
}

function verified(snapshot: TrustedComposerSnapshot): boolean {
  return snapshot.focused
    && snapshot.exactComposerVerified
    && snapshot.serviceId.length > 0
    && snapshot.accountId.length > 0
    && snapshot.conversationId.length > 0
    && snapshot.composerId.length > 0
    && Number.isSafeInteger(snapshot.hostGeneration)
    && snapshot.hostGeneration >= 0;
}

function immutableSnapshot(snapshot: TrustedComposerSnapshot): ImmutableTrustedComposerSnapshot {
  return Object.freeze({
    serviceId: snapshot.serviceId,
    accountId: snapshot.accountId,
    conversationId: snapshot.conversationId,
    composerId: snapshot.composerId,
    hostGeneration: snapshot.hostGeneration,
    focused: snapshot.focused,
    exactComposerVerified: snapshot.exactComposerVerified,
  });
}

/**
 * A small fail-closed state machine for future trusted composer adapters.
 * It never synthesizes input. Callers must execute `place`/`submit` only after
 * their native adapter independently revalidates the immutable snapshot carried
 * by each protected handoff decision.
 */
export class TrustedSendGuard {
  private armed: ArmedPlacement | null = null;

  cancel(): void {
    this.armed = null;
  }

  accept(mode: SendMode, event: TrustedEnter, snapshot: TrustedComposerSnapshot): GuardedSendDecision {
    if (!event.isTrusted) {
      this.cancel();
      return { action: "reject", reason: "untrusted" };
    }
    if (mode === "manual" || mode === "clipboard") {
      this.cancel();
      return { action: "copy" };
    }
    const protectedHandoff = immutableSnapshot(snapshot);
    if (!verified(protectedHandoff)) {
      this.cancel();
      return { action: "reject", reason: "unverified" };
    }
    if (mode === "single") {
      this.cancel();
      return { action: "submit", snapshot: protectedHandoff };
    }

    const armed = this.armed;
    if (!armed) {
      const expiresAtMs = event.occurredAtMs + DOUBLE_ENTER_WINDOW_MS;
      this.armed = { snapshot: protectedHandoff, eventId: event.eventId, expiresAtMs };
      return { action: "place", expiresAtMs, snapshot: protectedHandoff };
    }
    this.cancel();
    if (event.eventId === armed.eventId) return { action: "reject", reason: "repeated" };
    if (event.occurredAtMs > armed.expiresAtMs) return { action: "reject", reason: "expired" };
    if (!sameSnapshot(armed.snapshot, protectedHandoff)) return { action: "reject", reason: "changed" };
    return { action: "submit", snapshot: armed.snapshot };
  }
}
