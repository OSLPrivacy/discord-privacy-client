/**
 * The OSL Chat delivery runtime, extracted out of `main.ts` (T14-A0).
 *
 * `main.ts` is the single most contested file in the frontend, so the delivery
 * loop — the part of OSL Chat that decides *when* a message arrives — lives
 * here, behind a host interface, where it can be driven by a test without a DOM,
 * a Tauri runtime, or a real timer.
 *
 * T14-B2: delivery is NOT gated on the user standing on any particular screen.
 * A message must arrive while the app is open, on any route, without the user
 * navigating anywhere. Two things follow from that:
 *
 *  - When no conversation is open the runtime drains verified friends in a
 *    rotating batch (see `OSL_CHAT_DELIVERY_BATCH_LIMIT`), regardless of route.
 *  - When a conversation IS open, the runtime re-drains that conversation on
 *    the same cadence instead of sitting idle. The backend holds exactly one
 *    active OSL Chat context, so the open conversation is drained through the
 *    context that is already active and no other friend's context is activated
 *    for that tick — activating one would clobber the open chat.
 */

import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";
import type { OslChatHistoryRow } from "./adapters";
import type { OslChatMessage } from "./osl-chats-view";

/**
 * Friends drained per tick when no conversation is open.
 *
 * This used to be a hard `slice(0, 32)`: friend 33 never received anything and
 * nothing said so. It is now a *batch* size, not a cap — the runtime keeps a
 * cursor and advances it by the number of friends it attempted, so every
 * verified friend is covered across consecutive ticks. The bound exists only so
 * one tick cannot run unboundedly long; it never excludes anybody permanently.
 */
export const OSL_CHAT_DELIVERY_BATCH_LIMIT = 32;

/** Default cadence between drains. Kept from the pre-extraction scheduler. */
export const OSL_CHAT_DELIVERY_INTERVAL_MS = 30_000;

/** Retained history/timeline depth, matching the pre-extraction behaviour. */
const OSL_CHAT_MESSAGE_RETENTION = 200;

export interface OslChatDeliveryPerson {
  personId: string;
  safetyNumberVerified: boolean;
  pendingKeyChange: boolean;
}

export interface OslChatDeliveryContext {
  personId: string;
  peerOslUserId: string;
  scopeApproved: boolean;
}

export interface OslChatDeliveryHost {
  /** Identity must be loaded before any signed inbox call is made. */
  identityLoaded(): boolean;
  /**
   * A foreign protected context or host window owns the session (Discord
   * overlay, embedded host, native host). Activating an OSL Chat context would
   * clobber it, so the tick is skipped. This is a *session* precondition, never
   * a route check.
   */
  foreignContextActive(): boolean;
  /** The conversation the user has open, or null. */
  openConversationId(): string | null;
  /** True while a user-driven chat operation (send, refresh, open) is in flight. */
  conversationBusy(): boolean;
  /** Every friend eligible for delivery, in a stable order. */
  friends(): readonly OslChatDeliveryPerson[];
  /** Capture resistance must be on before any plaintext crosses IPC. */
  requestCaptureProtection(): Promise<boolean>;
  activateContext(personId: string): Promise<OslChatDeliveryContext | null>;
  closeContext(): Promise<boolean>;
  drainInbox(): Promise<NativeDiscordOverlayOpenedBatch | null>;
  loadHistory(): Promise<OslChatHistoryRow[] | null>;
  /**
   * @param background `true` for a conversation the user is not looking at
   * (raises unread + a local alert); `false` for the open conversation, whose
   * new messages render in place.
   */
  commitBatch(personId: string, batch: NativeDiscordOverlayOpenedBatch, background: boolean): void;
  commitHistory(personId: string, rows: readonly OslChatHistoryRow[], context: OslChatDeliveryContext): void;
}

export interface OslChatDeliveryRuntimeOptions {
  batchLimit?: number;
  intervalMs?: number;
  setTimer?: (callback: () => void, delayMs: number) => number;
  clearTimer?: (handle: number) => void;
}

export interface OslChatDeliveryRuntime {
  /** Run one delivery tick now. Never overlaps with another in-flight tick. */
  sync(): Promise<void>;
  /** Begin the self-rescheduling cadence. */
  start(delayMs?: number): void;
  /** Stop the cadence. A tick already in flight still completes. */
  stop(): void;
}

/**
 * Verified friends only. The trust gate is unchanged from the pre-extraction
 * loop: a friend must have a compared safety number and no pending key change.
 */
export function eligibleOslChatFriends(
  people: readonly OslChatDeliveryPerson[],
): readonly OslChatDeliveryPerson[] {
  return people.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
}

/**
 * The slice of friends this tick attempts, starting at `cursor` and wrapping.
 * Returns at most `limit` DISTINCT friends — a roster smaller than the limit is
 * covered exactly once, never repeated within a tick.
 */
export function oslChatDeliveryWindow<T>(
  people: readonly T[],
  cursor: number,
  limit: number,
): readonly T[] {
  if (!people.length || limit <= 0) return [];
  const take = Math.min(limit, people.length);
  const start = ((cursor % people.length) + people.length) % people.length;
  const window: T[] = [];
  for (let offset = 0; offset < take; offset += 1) window.push(people[(start + offset) % people.length]!);
  return window;
}

/** Map a decrypted history page onto the rendered timeline. Pure. */
export function oslChatHistoryMessages(
  rows: readonly OslChatHistoryRow[],
  context: OslChatDeliveryContext,
  formatTimestamp: (epochSeconds: number) => string,
): OslChatMessage[] {
  return rows.slice().reverse().map((row) => {
    const incoming = row.senderOslUserId === context.peerOslUserId;
    return {
      messageId: row.messageId,
      direction: incoming ? "incoming" as const : "outgoing" as const,
      body: row.plaintext,
      state: incoming ? "received" as const : "sent" as const,
      timestampLabel: formatTimestamp(row.decryptedAt),
    };
  });
}

/**
 * History replaces the durable timeline; view-once messages already opened on
 * this device are not in history and must survive the replacement. Pure.
 */
export function mergeOslChatTimeline(
  durable: readonly OslChatMessage[],
  openedViewOnce: readonly OslChatMessage[],
): OslChatMessage[] {
  return [...durable, ...openedViewOnce].slice(-OSL_CHAT_MESSAGE_RETENTION);
}

export function createOslChatDeliveryRuntime(
  host: OslChatDeliveryHost,
  options: OslChatDeliveryRuntimeOptions = {},
): OslChatDeliveryRuntime {
  const batchLimit = options.batchLimit ?? OSL_CHAT_DELIVERY_BATCH_LIMIT;
  const intervalMs = options.intervalMs ?? OSL_CHAT_DELIVERY_INTERVAL_MS;
  const setTimer = options.setTimer ?? ((callback, delayMs) => window.setTimeout(callback, delayMs));
  const clearTimer = options.clearTimer ?? ((handle: number) => window.clearTimeout(handle));

  let busy = false;
  let running = false;
  let timer: number | null = null;
  let cursor = 0;

  /**
   * Drain the conversation the user has open, through the context that is
   * already active. No activate/close: doing either would tear down the open
   * chat's own context.
   */
  async function drainOpenConversation(personId: string): Promise<void> {
    if (host.conversationBusy()) return;
    if (!await host.requestCaptureProtection()) return;
    // The user may have sent or navigated while protection was being applied.
    if (host.conversationBusy() || host.openConversationId() !== personId) return;
    const batch = await host.drainInbox();
    if (!batch) return;
    if (host.openConversationId() !== personId) return;
    host.commitBatch(personId, batch, false);
  }

  async function drainRoster(): Promise<void> {
    const people = eligibleOslChatFriends(host.friends());
    if (!people.length) return;
    // A sender can require capture protection. Apply it before asking any
    // approved friend inbox to return plaintext, even for background delivery.
    if (!await host.requestCaptureProtection()) return;
    const window = oslChatDeliveryWindow(people, cursor, batchLimit);
    // Advance first: a tick that aborts part way must not re-start on the same
    // friends next time, or later friends starve exactly as the old cap did.
    cursor = (cursor + window.length) % people.length;
    for (const person of window) {
      // Delivery is never route-gated (T14-B2). It still yields to a foreign
      // protected context or to the user opening a conversation, because both
      // own the single active OSL Chat context.
      if (host.foreignContextActive() || host.openConversationId()) break;
      const context = await host.activateContext(person.personId);
      if (!context) continue;
      try {
        if (!context.scopeApproved) continue;
        const batch = await host.drainInbox();
        if (batch) host.commitBatch(person.personId, batch, true);
        const history = await host.loadHistory();
        if (history) host.commitHistory(person.personId, history, context);
      } finally {
        await host.closeContext();
      }
    }
  }

  async function sync(): Promise<void> {
    if (busy || host.foreignContextActive() || !host.identityLoaded()) return;
    busy = true;
    try {
      const open = host.openConversationId();
      if (open) await drainOpenConversation(open);
      else await drainRoster();
    } finally {
      busy = false;
    }
  }

  function schedule(delayMs: number): void {
    timer = setTimer(() => {
      timer = null;
      void sync().finally(() => { if (running) schedule(intervalMs); });
    }, delayMs);
  }

  return {
    sync,
    start(delayMs = intervalMs): void {
      if (running) return;
      running = true;
      schedule(delayMs);
    },
    stop(): void {
      running = false;
      if (timer !== null) { clearTimer(timer); timer = null; }
    },
  };
}
