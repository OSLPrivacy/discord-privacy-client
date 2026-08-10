import type { VerificationWarningSurface } from "./verification-warning";
import { viewOnceControlMarkup } from "./view-once-tier";

// Matches the enforced OSL Chat logical-message limit in broker.rs.
export const OSL_CHAT_MAX_DRAFT_BYTES = 1024 * 1024;

export const FIRST_PARTY_OSL_SERVICE_SURFACES = [{
  surfaceId: "osl-chat",
  destination: "Inbox",
  displayName: "OSL Chat",
  participantScope: "verified_friend",
  accountScope: "active_friend_only",
  plaintextBoundary: "osl_controlled_composer",
  sendAuthority: "explicit_user_action",
  historyVisibility: "local_osl_history",
  externalProvider: false,
}] as const;

export type FirstPartyOslServiceSurface =
  typeof FIRST_PARTY_OSL_SERVICE_SURFACES[number];

export type FirstPartyOslServiceSurfaceId =
  FirstPartyOslServiceSurface["surfaceId"];

export const OSL_CHAT_DELIVERY_STATES = [
  "queued",
  "sent",
  "delivered",
  "received",
  "opened",
  "expired",
  "failed",
] as const;

export type OslChatDeliveryState = typeof OSL_CHAT_DELIVERY_STATES[number];

export type OslChatMessageDirection = "outgoing" | "incoming";

export const OSL_PRIMARY_DESTINATIONS = [
  "Home",
  "Inbox",
  "People",
  "Privacy",
  "Activity",
  "Connections",
] as const;

export type OslPrimaryDestination = typeof OSL_PRIMARY_DESTINATIONS[number];

export type FirstPartyOslSurfaceState = "available" | "coming_later" | "unavailable";

export interface FirstPartyOslSurfaceContract {
  id: "osl-chat" | "osl-enclaves" | "osl-mail";
  label: "OSL Chat" | "OSL Enclaves" | "OSL Mail";
  destination: "Inbox";
  state: FirstPartyOslSurfaceState;
  primaryAction: string;
  protectionScope: string;
  externalPlatform: false;
  requiresConnectedService: false;
  missingCapability: "unavailable";
}

export const FIRST_PARTY_OSL_SURFACES: readonly FirstPartyOslSurfaceContract[] = [
  {
    id: "osl-chat",
    label: "OSL Chat",
    destination: "Inbox",
    state: "available",
    primaryAction: "Start a private conversation",
    protectionScope: "OSL-owned conversations",
    externalPlatform: false,
    requiresConnectedService: false,
    missingCapability: "unavailable",
  },
  {
    id: "osl-enclaves",
    label: "OSL Enclaves",
    destination: "Inbox",
    state: "available",
    primaryAction: "Create an enclave",
    protectionScope: "OSL-owned enclaves",
    externalPlatform: false,
    requiresConnectedService: false,
    missingCapability: "unavailable",
  },
  {
    id: "osl-mail",
    label: "OSL Mail",
    destination: "Inbox",
    state: "coming_later",
    primaryAction: "Open mail",
    protectionScope: "OSL-owned mail",
    externalPlatform: false,
    requiresConnectedService: false,
    missingCapability: "unavailable",
  },
] as const;

export function firstPartyOslSurfaceContracts(): readonly FirstPartyOslSurfaceContract[] {
  return FIRST_PARTY_OSL_SURFACES;
}

export function firstPartyOslSurfaceContract(
  id: FirstPartyOslSurfaceContract["id"],
): FirstPartyOslSurfaceContract {
  return FIRST_PARTY_OSL_SURFACES.find((surface) => surface.id === id)!;
}

export interface OslChatFriend {
  personId: string;
  nickname: string;
  verified: boolean;
  ready: boolean;
  preview: string | null;
  previewVisible: boolean;
  unreadCount: number;
  /**
   * Local evidence that this person has completed their half of the symmetric
   * handshake: something of theirs has been decrypted here. The sender cannot
   * observe the peer's device, so this is the only honest signal available and
   * its absence is NOT proof of anything — it only means OSL has never seen the
   * peer answer. Absent (the default), an outgoing message must not be
   * presented as a finished, readable delivery.
   */
  handshakeConfirmed?: boolean;
  /**
   * TASK0171's `verification_state` for this person's OSL allowed-place
   * direction, collapsed to a boolean: `true` only when both directions are
   * saved (two-way). One-way or absent allowances must never draw the tick --
   * a one-way allowance is not a reciprocal, verified relationship.
   */
  verificationTwoWay?: boolean;
  /** The peer's published key no longer matches the one this chat trusted. */
  pendingKeyChange?: boolean;
  /** Presence is a local hint only; it is never used as a send permission. */
  online?: boolean;
  /** A terse sidebar timestamp supplied by the host, when one is available. */
  timeLabel?: string;
  /**
   * The owner picture this row is allowed to draw, as answered by the protected
   * picture query for the signed-in friend (task 0237). Unset or null means the
   * query did not grant a picture to this reader, and the row draws the stable
   * coloured initial instead. The list payload never carries the image itself.
   */
  picture?: string | null;
}

/** Exact refusal shown everywhere an unreviewed friend key blocks a send. */
export const OSL_CHAT_KEY_CHANGED_REFUSAL_REASON = "Sending is blocked until you verify the new safety number.";

export interface OslChatMessage {
  messageId: string;
  direction: OslChatMessageDirection;
  body: string;
  state: OslChatDeliveryState;
  timestampLabel: string;
  /**
   * The local calendar-day caption for this message. It is carried alongside
   * the quiet time so the thread never has to guess a day from localized text.
   */
  dateLabel?: string;
  reactions?: readonly OslChatMessageReaction[];
  expiresAt?: number;
}

export interface OslChatMessageReaction {
  emoji: string;
  count: number;
  mine: boolean;
}

export interface OslChatReactionToggleResult {
  emoji: string;
  added: boolean;
  removed: boolean;
}

/**
 * TASK 5011. Pure reducer for a reaction chip tap: applies the server's
 * add/remove result to the message's reaction list so the tapped chip's
 * count moves by exactly one and its "mine" mark flips with it.
 */
export function applyOslChatReactionToggle(
  reactions: readonly OslChatMessageReaction[],
  result: OslChatReactionToggleResult,
): OslChatMessageReaction[] {
  const next = [...reactions];
  const index = next.findIndex((reaction) => reaction.emoji === result.emoji);
  if (result.removed) {
    if (index >= 0) {
      const current = next[index]!;
      const count = Math.max(0, current.count - 1);
      if (count === 0) next.splice(index, 1);
      else next[index] = { ...current, count, mine: false };
    }
  } else if (result.added) {
    if (index >= 0) {
      const current = next[index]!;
      next[index] = { ...current, count: current.count + 1, mine: true };
    } else {
      next.push({ emoji: result.emoji, count: 1, mine: true });
    }
  }
  return next;
}

export interface OslChatBuildWarning {
  kind: "changedBuild";
  reason: "changed" | "corruptProof";
  message: string;
  messageSendingAvailable: true;
}

export interface OslChatGroup {
  groupId: string;
  name: string;
  memberCount: number;
  memberIds: readonly string[];
}

export interface OslGroupChatHeaderAction {
  id: string;
  label: string;
  disabled?: boolean;
}

export interface OslGroupChatHeaderModel {
  groupName: string;
  memberCount: number;
  actions: readonly OslGroupChatHeaderAction[];
}

export interface OslChatsViewModel {
  friends: readonly OslChatFriend[];
  activePersonId: string | null;
  messages: readonly OslChatMessage[];
  draft: string;
  busy: boolean;
  viewOnce?: boolean;
  /**
   * TASK 0594. Whether this account may *create* a view once message (Pro, per
   * 0590). Absent means Free: the composer control is drawn off and says why.
   * Opening one is never gated by this -- that path is free (0591).
   */
  viewOnceCreationAllowed?: boolean;
  /** Where a verified-peer warning is permitted to be shown for this view. */
  verificationWarningSurface?: VerificationWarningSurface;
  homeLogoUrl?: string;
  /**
   * Remote attachment copies OSL asked the relay to delete and could NOT
   * confirm gone -- `DeletionDrainReport::retained`, delivered by
   * `osl://attachment-deletion-unconfirmed` (see `main.ts`). Absent/0 means
   * "nothing is known to be owed", which is not the same as "confirmed gone";
   * that is why the composer copy never promises removal on its own.
   */
  deletionUnconfirmed?: number;
  buildIntegrity?: OslChatBuildIntegrityStatus | null;
  buildWarning?: OslChatBuildWarning | null;
  /** The display name from the local OSL profile. Never render a literal “You”. */
  profileDisplayName?: string;
  conversationFilter?: "direct" | "groups" | "enclaves";
  searchQuery?: string;
  sendBlockedReason?: string | null;
  attachmentAvailable?: boolean;
}

export type OslChatBuildIntegrityStatus = "verified" | "mismatch" | "unknown";

/**
 * Which sender-facing receipt state the latest outgoing message has actually
 * earned, or `null` when there is nothing to report.
 *
 * D-136. This exists because the receipt line used to branch on
 * `"delivered"`/`"opened"`/`"expired"` and fall through to `"Prepared"` -- and
 * only the fall-through could ever run, so "Delivery receipt -- Not confirmed"
 * was the single reachable value on every message anyone would ever send. The
 * branches are now exactly the states a producer sets, each cited:
 *
 * * `"received"` -- the recipient app posts a signed `Received` acknowledgment
 *   for a view-once message (`apps/osl-hub/src/broker.rs:5659` builds it,
 *   `:4726`/`:4766`/`:4874`/`:4943` post it), the sender's inbox drain
 *   authenticates and records it (`broker.rs:4427-4485`), it reaches the
 *   renderer as `NativeDiscordOverlayAcknowledgment.status`
 *   (`overlay-state.ts:143-145`) and `main.ts` `commitOslChatBatch` applies it
 *   to the matching outgoing message by `messageId`. This is the fact the
 *   product genuinely knows and used to throw away.
 * * `"opened"` -- the same renderer path admits it, but production deliberately
 *   never emits an Opened acknowledgment: `broker.rs:4791-4794` retires the
 *   relay row "without publishing an Opened acknowledgment ... until a durable
 *   mutual-consent grant exists", pinned by
 *   `d7_production_has_no_opened_acknowledgment_emission_branch`
 *   (`broker.rs:12531`). The branch is kept because the plumbing is complete
 *   and the suppression is a peer-side consent policy, not a missing producer.
 *
 * `"delivered"` and `"expired"` have NO producer anywhere in the tree -- no
 * Rust path, no adapter and no renderer assignment ever sets them on a message
 * -- so branching on them was decoration and they are gone from here. They stay
 * in {@link OSL_CHAT_DELIVERY_STATES} because that union is the per-message
 * label domain (`deliveryLabel`), not this status surface.
 *
 * The privacy property is unchanged and is the reason this returns `null`
 * rather than a distinct "we have not heard anything" state: every state
 * without a receipt collapses to one caller-side wording in
 * `receipt-status.ts`, so a recipient who has receipts off is indistinguishable
 * from one whose message has not arrived.
 */
export function senderReceiptStateFor(
  messages: readonly OslChatMessage[],
): "Delivered" | "Opened" | null {
  const latestOutgoing = [...messages].reverse().find((message) => message.direction === "outgoing");
  if (latestOutgoing?.state === "opened") return "Opened";
  if (latestOutgoing?.state === "received") return "Delivered";
  return null;
}

export function firstPartyOslServiceSurface(
  surfaceId: FirstPartyOslServiceSurfaceId,
): FirstPartyOslServiceSurface {
  return FIRST_PARTY_OSL_SERVICE_SURFACES.find((surface) => surface.surfaceId === surfaceId)!;
}

const settingsIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></svg>';
const onceIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2.2 12s3.5-5.5 9.8-5.5S21.8 12 21.8 12 18.3 17.5 12 17.5 2.2 12 2.2 12Z"/><circle cx="12" cy="12" r="2.7"/></svg>';
const sendIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>';
const attachIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8.6 12.8 5.9-5.9a3.2 3.2 0 0 1 4.5 4.5l-7.8 7.8a5.1 5.1 0 1 1-7.2-7.2l7.5-7.5"/></svg>';
const emojiIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5"/><path d="M8.5 14.5c.9 1.1 2.1 1.6 3.5 1.6s2.6-.5 3.5-1.6M8.5 9.5h.01M15.5 9.5h.01"/></svg>';
const composeIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 20 4.1-1 9.8-9.8a2.1 2.1 0 0 0-3-3L5.1 16 4 20Z"/><path d="m13.5 7.5 3 3"/></svg>';
const moreIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/></svg>';
const verificationTickIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 13 4 4 10-10"/></svg>';

/**
 * TASK0175. The tick is the two-way whitelist relationship becoming
 * verified (TASK0171) -- it must be absent for `none`, `one-way`, and
 * `undefined`, and present only once both allowed-place directions exist.
 */
export function oslVerificationTickMarkup(twoWay: boolean | undefined): string {
  if (twoWay !== true) return "";
  return `<span class="osl-verification-tick" data-osl-verification-tick="visible" title="Verified">${verificationTickIcon}</span>`;
}

function initials(value: string): string {
  return value.split(/\s+/u).filter(Boolean).map((part) => part[0]).join("").slice(0, 2).toUpperCase() || "?";
}

function avatar(value: string, className = "", online = false, picture: string | null = null): string {
  if (picture) {
    return `<span class="osl-chat-avatar-shell"><img class="osl-chat-avatar friend-picture ${className}" src="${escapeHtml(picture)}" alt="" aria-hidden="true"/>${online ? '<i class="osl-chat-online"></i>' : ""}</span>`;
  }
  return `<span class="osl-chat-avatar ${className}" aria-hidden="true">${escapeHtml(initials(value))}${online ? '<i class="osl-chat-online"></i>' : ""}</span>`;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

export function oslChatDraftBytes(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

export function applyOslChatDraftToElement(
  draftElement: Pick<HTMLTextAreaElement, "value"> | null,
  draft: string,
): void {
  if (draftElement && draftElement.value !== draft) draftElement.value = draft;
}

export function submitsOslChatDraft(event: {
  key: string;
  isComposing?: boolean;
  altKey?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
}): boolean {
  if (event.key !== "Enter") return false;
  if (event.isComposing) return false;
  return !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
}

function deliveryLabel(state: OslChatDeliveryState): string {
  switch (state) {
    case "queued": return "Not sent";
    case "sent": return "Sent";
    case "delivered": return "Delivered";
    case "received": return "Received";
    case "opened": return "Opened";
    case "expired": return "Expired";
    case "failed": return "Failed";
  }
}

function friendPreview(friend: OslChatFriend): string {
  if (!friend.previewVisible) {
    return '<span class="osl-chat-friend-preview is-hidden">Preview hidden</span>';
  }
  if (!friend.preview) {
    return '<span class="osl-chat-friend-preview is-empty">No messages yet</span>';
  }
  return `<span class="osl-chat-friend-preview">${escapeHtml(friend.preview)}</span>`;
}

function friendRow(friend: OslChatFriend, activePersonId: string | null, busy: boolean): string {
  const active = friend.personId === activePersonId;
  const keyChanged = friend.pendingKeyChange === true;
  const preview = friend.previewVisible && friend.preview
    ? `<span class="osl-chat-friend-preview">${escapeHtml(friend.preview)}</span>`
    : friendPreview(friend);
  return `<div class="osl-chat-friend${active ? " is-active" : ""}${keyChanged ? " is-key-changed" : ""}" data-person-id="${escapeHtml(friend.personId)}">
    <button class="osl-chat-friend-open" type="button" data-osl-chat-open="${escapeHtml(friend.personId)}" ${active ? 'aria-current="true"' : ""} ${busy ? 'disabled aria-disabled="true"' : ""}>
      ${avatar(friend.nickname, "", friend.online !== false, friend.picture ?? null)}<span class="osl-chat-friend-copy"><strong class="${keyChanged ? "is-key-changed" : ""}">${escapeHtml(friend.nickname)}${keyChanged ? '<span class="osl-chat-key-changed-triangle" role="img" aria-label="Safety number changed">⚠</span>' : oslVerificationTickMarkup(friend.verificationTwoWay)}</strong>${preview}</span><span class="osl-chat-friend-end"><time class="osl-chat-status-style">${escapeHtml(friend.timeLabel ?? "")}</time>${friend.unreadCount > 0 ? `<b class="osl-chat-unread osl-chat-status-style" aria-label="${friend.unreadCount} unread">${Math.min(friend.unreadCount, 99)}</b>` : ""}</span>
    </button>
  </div>`;
}

/**
 * An outgoing message really is end-to-end encrypted and really did upload.
 * What it is not, until the peer has answered at least once, is readable:
 * a peer who has not added your invite, verified you and turned this chat on
 * has no binding and cannot decrypt. Saying "Sent" alone tells the user the
 * message landed. It did not.
 */
/**
 * The only local proof that a peer completed their half of the symmetric
 * handshake: something of theirs decrypted here. An outgoing message proves
 * nothing -- encrypting and uploading succeed whether or not the peer ever
 * bound a context. The timeline is seeded from the durable local history when a
 * chat opens, so this survives a restart.
 */
export function oslChatHandshakeConfirmed(messages: readonly OslChatMessage[]): boolean {
  return messages.some((message) => message.direction === "incoming");
}

export function oslChatMessageUnreadableNote(
  message: OslChatMessage,
  handshakeConfirmed: boolean,
): string {
  if (message.direction !== "outgoing" || handshakeConfirmed) return "";
  if (message.state !== "sent" && message.state !== "delivered") return "";
  return "Not readable yet";
}

export function oslChatHandshakeWarning(friend: OslChatFriend): string {
  if (!friend.verified || friend.handshakeConfirmed === true) return "";
  return `Nothing has ever arrived from ${friend.nickname}, so OSL cannot tell whether they finished their half. Until they add your invite, verify you and turn this chat on, what you send here cannot be opened on their device. Both people must complete every step.`;
}

function oslChatMutualReadiness(friend: OslChatFriend): boolean {
  return friend.verified && friend.ready && friend.handshakeConfirmed === true;
}

function messageRow(message: OslChatMessage, friend: OslChatFriend, profileDisplayName: string, continuation: boolean): string {
  const author = message.direction === "outgoing" ? profileDisplayName : friend.nickname;
  const label = deliveryLabel(message.state);
  const unreadable = oslChatMessageUnreadableNote(message, friend.handshakeConfirmed === true);
  const reactionButtons = [
    ...(message.reactions ?? []).map((reaction) => `<button class="osl-chat-reaction${reaction.mine ? " is-mine" : ""}" type="button" data-osl-chat-reaction="${escapeHtml(message.messageId)}" data-osl-chat-emoji="${escapeHtml(reaction.emoji)}" data-osl-chat-reaction-mine="${reaction.mine ? "true" : "false"}" aria-pressed="${reaction.mine ? "true" : "false"}"><span>${escapeHtml(reaction.emoji)}</span><span>${reaction.count}</span></button>`),
  ].join("");
  return `<article class="osl-chat-message is-${message.direction}${continuation ? " is-continuation" : ""}" data-message-id="${escapeHtml(message.messageId)}" data-message-author="${escapeHtml(author)}">
    <p class="osl-chat-message-text">${escapeHtml(message.body)}</p>
    ${reactionButtons ? `<div class="osl-chat-reactions">${reactionButtons}</div>` : ""}
    <footer><time class="osl-chat-message-timestamp osl-chat-status-style">${escapeHtml(message.timestampLabel)}</time><span class="osl-chat-message-state osl-chat-status-style is-${message.state}">${label}</span>${unreadable ? `<span class="osl-chat-message-unreadable osl-chat-status-style">${escapeHtml(unreadable)}</span>` : ""}</footer>
  </article>`;
}

/** Render consecutive messages from one sender as one visual group. */
function messageGroups(messages: readonly OslChatMessage[], friend: OslChatFriend, profileDisplayName: string): string {
  let markup = "";
  let previous: OslChatMessage | null = null;
  let group: OslChatMessage[] = [];

  const appendGroup = (): void => {
    if (!group.length) return;
    const first = group[0]!;
    const sender = first.direction === "outgoing" ? profileDisplayName : friend.nickname;
    markup += `<section class="osl-chat-message-group is-${first.direction}" data-osl-chat-message-group="${first.direction}" data-message-count="${group.length}">
      ${avatar(sender, "is-message")}<div class="osl-chat-message-group-content"><header class="osl-chat-message-group-meta"><strong>${escapeHtml(sender)}</strong></header>${group.map((message, index) => messageRow(message, friend, profileDisplayName, index > 0)).join("")}</div>
    </section>`;
    group = [];
  };

  for (const message of messages) {
    if (previous && message.dateLabel && previous.dateLabel && message.dateLabel !== previous.dateLabel) {
      appendGroup();
      markup += `<div class="osl-chat-date-divider" role="separator"><span>${escapeHtml(message.dateLabel)}</span></div>`;
    } else if (previous && message.direction !== previous.direction) {
      appendGroup();
    }
    group.push(message);
    previous = message;
  }
  appendGroup();
  return markup;
}

/**
 * D-135 / master §7.5 -- "requested deletion is never displayed as verified
 * deletion", in the second place it was broken.
 *
 * `DeletionDrainReport::retained` counts remote attachment copies OSL asked the
 * cipher store to delete and could not confirm gone
 * (`apps/osl-hub/src/peer_attachment_io.rs:780-790`). That count was computed
 * correctly and dropped on the floor by `.map(|_report| ())` in
 * `native_attachment_transport.rs`, so the only evidence a copy might still be
 * on the relay never left Rust while the composer promised removal.
 *
 * Two states, not three. "Nothing owed that OSL knows of" renders nothing at
 * all; "we asked and cannot confirm" renders this. There is deliberately no
 * third, friendlier word for the second case, and nothing here may ever state
 * removal as an accomplished fact -- the drain is still retrying, and a request
 * is not a confirmation.
 *
 * The wording says "the copy that was sent", never where it is being kept. This
 * view is barred by `osl-chats-view.test.ts` from naming transport internals --
 * no "relay", no "server" -- which is why the status attribute here is
 * `data-deletion-status` rather than the `data-server-status` that
 * `destruct-status.ts` uses on surfaces that are allowed to say it. The value
 * vocabulary is deliberately the same one.
 */
function deletionUnconfirmedRow(count: number): string {
  if (!Number.isSafeInteger(count) || count <= 0) return "";
  const copies = count === 1 ? "1 attachment copy" : `${count.toLocaleString("en-US")} attachment copies`;
  return `<p class="osl-chat-deletion-unconfirmed warning" role="status" data-osl-deletion-unconfirmed="${count}" data-deletion-status="not-confirmed"><strong>Deletion was not confirmed</strong><small>OSL asked for ${copies} it sent to be deleted, and has not been able to confirm it. It keeps trying. Until it can confirm, assume the copy is still there.</small></p>`;
}

function buildIntegrityWarningRow(status: OslChatBuildIntegrityStatus | null | undefined): string {
  if (status === "verified" || !status) return "";
  const detail = status === "mismatch"
    ? "This app copy does not match OSL's signed build list. You can still send messages, but update or reinstall OSL before trusting this build."
    : "OSL could not verify this app copy against its signed build list. You can still send messages, but update or reinstall OSL before trusting this build.";
  return `<p class="osl-chat-build-warning warning" role="status" data-osl-build-integrity="${status}"><strong>Build verification warning</strong><small>${escapeHtml(detail)}</small></p>`;
}

function buildWarningRow(warning: OslChatBuildWarning | null | undefined): string {
  if (!warning || warning.kind !== "changedBuild" || warning.messageSendingAvailable !== true) {
    return "";
  }
  const reason = warning.reason === "corruptProof" ? "corrupt-proof" : "changed";
  return `<p class="osl-chat-build-warning warning" role="status" data-osl-chat-build-warning="${reason}" data-message-sending-available="true"><strong>Changed build warning</strong><small>${escapeHtml(warning.message)}</small></p>`;
}

function emptyThread(): string {
  return `<section class="osl-chat-thread is-empty" aria-label="OSL direct chat">
    <div><strong>Select a conversation</strong><p>Choose someone from your chats to read or write a message.</p></div>
  </section>`;
}

function activeThread(model: OslChatsViewModel, friend: OslChatFriend): string {
  const bytes = oslChatDraftBytes(model.draft);
  const withinLimit = bytes <= OSL_CHAT_MAX_DRAFT_BYTES;
  const hasDraft = model.draft.trim().length > 0;
  const mutualReady = oslChatMutualReadiness(friend);
  const keyChanged = friend.pendingKeyChange === true;
  const peerState = mutualReady && !keyChanged ? "mutual" : "one-way";
  const canSend = hasDraft && withinLimit && !model.busy && !keyChanged;
  const readiness = keyChanged
    ? "Verify this key change before sending."
    : !friend.verified
    ? "Verify this friend before sending."
    : !friend.ready
      ? "Chat is not ready."
      : !mutualReady
        ? "Chat needs both people to answer before sending."
      : "";
  const profileDisplayName = model.profileDisplayName?.trim() || "OSL profile";
  const messages = model.messages.length
    ? `${messageGroups(model.messages, friend, profileDisplayName)}<div class="osl-chat-typing" aria-label="${escapeHtml(friend.nickname)} is typing">${avatar(friend.nickname, "is-typing")}<span><i></i><i></i><i></i></span></div>`
    : '<p class="osl-chat-thread-empty">No messages yet.</p>';
  const warningSurface = model.verificationWarningSurface ?? "conversation-open";
  const handshakeWarning = warningSurface === "none" ? "" : oslChatHandshakeWarning(friend);
  const unconfirmed = handshakeWarning
    ? `<p class="osl-chat-handshake-warning is-${warningSurface}" role="status" data-verification-warning-surface="${warningSurface}">${escapeHtml(handshakeWarning)}</p>`
    : "";
  const verification = keyChanged
    ? "Key changed — check before sending"
    : friend.verified ? "Verified · online" : "Not verified yet";
  const keyBanner = keyChanged
    ? `<aside class="osl-chat-key-banner osl-chat-key-changed-banner" role="alert" data-osl-key-changed-banner="true"><span><strong>Key changed</strong><small>${escapeHtml(OSL_CHAT_KEY_CHANGED_REFUSAL_REASON)}</small></span><button type="button" data-verify-person="${escapeHtml(friend.personId)}">Verify new safety number</button></aside>`
    : "";
  const blocked = model.sendBlockedReason
    ? `<aside class="osl-chat-blocked-panel" role="alert"><strong>Message not sent</strong><p>${escapeHtml(model.sendBlockedReason)}</p><button type="button" data-osl-chat-blocked-close>Back to chat</button></aside>`
    : "";
  return `<section class="osl-chat-thread" aria-label="OSL direct chat with ${escapeHtml(friend.nickname)}">
    <header class="osl-chat-thread-header">${avatar(friend.nickname, "is-thread", friend.online !== false, friend.picture ?? null)}<button class="osl-chat-thread-identity" type="button" data-open-safety-number="${escapeHtml(friend.personId)}"><h2>${escapeHtml(friend.nickname)}${oslVerificationTickMarkup(friend.verificationTwoWay)}</h2><span class="osl-chat-status-style is-${friend.pendingKeyChange ? "key-changed" : friend.verified ? "verified" : "unverified"}">${verification}</span></button><button class="osl-chat-thread-settings" type="button" data-osl-chat-settings="${escapeHtml(friend.personId)}" aria-label="Chat settings">${moreIcon}</button></header>
    ${keyBanner}${unconfirmed}${blocked}
    <div class="osl-chat-message-list" role="log" aria-live="polite" aria-relevant="additions text">${messages}</div>
    ${buildIntegrityWarningRow(model.buildIntegrity)}
    ${buildWarningRow(model.buildWarning)}
    ${deletionUnconfirmedRow(model.deletionUnconfirmed ?? 0)}
    <form class="osl-chat-composer${keyChanged ? " is-key-blocked" : ""}" data-osl-chat-compose="${escapeHtml(friend.personId)}"${keyChanged ? ' aria-disabled="true"' : ""}>
      <label for="osl-chat-draft">Message</label>
      <div class="osl-chat-composer-bar"><button class="osl-chat-attach" id="osl-chat-attach" type="button" aria-label="Attach a file" ${model.attachmentAvailable && !model.busy && !keyChanged ? "" : 'disabled title="Attachments are available in this approved chat with OSL Pro"'}>${attachIcon}</button><textarea id="osl-chat-draft" rows="1" placeholder="${keyChanged ? "Verify the key change before you send anything" : `Message ${escapeHtml(friend.nickname)}`}" autocomplete="off" spellcheck="true" aria-describedby="osl-chat-draft-count osl-chat-readiness" ${keyChanged ? "disabled" : ""}>${escapeHtml(model.draft)}</textarea>${viewOnceControlMarkup({
        id: "osl-chat-view-once",
        layout: "composer",
        className: "osl-chat-view-once",
        title: "View once",
        checked: model.viewOnce === true,
        creationAllowed: model.viewOnceCreationAllowed === true,
        unavailable: keyChanged,
        // Deliberately not disabled while busy: this is a preference for the
        // *next* send, not an action, and the in-flight send already captured
        // its value. An off state here would need a reason nobody needs to
        // read, and the Send button is the real in-flight guard.
        iconMarkup: onceIcon,
        detail: "Making a view-once item needs Pro; opening one is free. Kept out of OSL history. OSL asks for the sent copy to be deleted once it is opened, and says here when it cannot confirm that.",
      })}<button class="osl-chat-emoji" type="button" aria-label="Choose emoji" ${keyChanged ? "disabled" : ""}>${emojiIcon}</button><button class="osl-chat-send" type="submit" aria-label="${model.busy ? "Sending" : "Send"}" data-osl-chat-send-route="send-button" data-osl-chat-peer-state="${peerState}" data-osl-chat-send-context="${mutualReady && !keyChanged && !model.busy ? "1" : "0"}" ${canSend ? "" : "disabled"}>${sendIcon}<span>${model.busy ? "Sending…" : "Send"}</span></button><span class="osl-chat-drop-outline" aria-hidden="true">Drop to attach</span></div>
      <div class="osl-chat-composer-meta"><span id="osl-chat-readiness" class="osl-chat-readiness">${readiness}</span><output id="osl-chat-draft-count" class="osl-chat-byte-count${withinLimit ? "" : " is-over"}">${withinLimit ? "" : `${bytes.toLocaleString("en-US")} / ${OSL_CHAT_MAX_DRAFT_BYTES.toLocaleString("en-US")}`}</output></div>
    </form>
  </section>`;
}

export function oslGroupChatHeaderMarkup(model: OslGroupChatHeaderModel): string {
  const memberCountText = `${model.memberCount} member${model.memberCount === 1 ? "" : "s"}`;
  const actions = model.actions
    .map(
      (action) =>
        `<button class="osl-group-chat-action" type="button" data-osl-group-chat-action="${escapeHtml(action.id)}" ${action.disabled ? 'disabled aria-disabled="true"' : ""}>${escapeHtml(action.label)}</button>`,
    )
    .join("");
  return `<header class="osl-group-chat-header" aria-label="OSL group chat header">
    <div class="osl-group-chat-identity">
      <span class="osl-group-chat-avatar" aria-hidden="true">${escapeHtml(initials(model.groupName))}</span>
      <div>
        <h2 class="osl-group-chat-name">${escapeHtml(model.groupName)}</h2>
        <span class="osl-group-chat-member-count" data-osl-group-chat-member-count="${model.memberCount}">${escapeHtml(memberCountText)}</span>
      </div>
    </div>
    <div class="osl-group-chat-actions" aria-label="Chat actions">${actions}</div>
  </header>`;
}

export function oslGroupChatHeaderEmptyMarkup(): string {
  return `<header class="osl-group-chat-header is-empty" aria-label="OSL group chat header">
    <div class="osl-group-chat-identity">
      <span class="osl-group-chat-avatar is-empty" aria-hidden="true">?</span>
      <div>
        <h2 class="osl-group-chat-name">Select a group</h2>
        <span class="osl-group-chat-member-count" data-osl-group-chat-member-count="0">No group selected</span>
      </div>
    </div>
    <div class="osl-group-chat-actions" aria-label="Chat actions"></div>
  </header>`;
}

export function oslChatsViewMarkup(model: OslChatsViewModel): string {
  const activeFriend = model.activePersonId
    ? model.friends.find((friend) => friend.personId === model.activePersonId) ?? null
    : null;
  const matchingFriends = model.friends.filter((friend) => {
    const query = model.searchQuery?.trim().toLocaleLowerCase() ?? "";
    return !query || `${friend.nickname} ${friend.preview ?? ""}`.toLocaleLowerCase().includes(query);
  });
  const pinned = matchingFriends.slice(0, 1);
  const recent = matchingFriends.slice(1);
  const friends = matchingFriends.length
    ? `${pinned.length ? `<p class="osl-chat-list-label">Pinned</p>${pinned.map((friend) => friendRow(friend, model.activePersonId, model.busy)).join("")}` : ""}${recent.length ? `<p class="osl-chat-list-label">Recent</p>${recent.map((friend) => friendRow(friend, model.activePersonId, model.busy)).join("")}` : ""}`
    : '<p class="osl-chat-friends-empty">No friends yet.</p>';
  const filter = model.conversationFilter ?? "direct";
  const tabs = (["direct", "groups", "enclaves"] as const).map((item) => `<button type="button" data-osl-chat-filter="${item}" aria-pressed="${filter === item}">${item === "direct" ? "Direct" : item[0]!.toUpperCase() + item.slice(1)}</button>`).join("");
  const profileName = model.profileDisplayName?.trim() || "OSL profile";
  return `<div class="osl-chats-view">
    <aside class="osl-chat-friends" aria-label="Chats"><header><h1>Chats</h1><button class="osl-chat-new" type="button" data-osl-chat-new aria-label="Start something">${composeIcon}</button></header><div class="osl-chat-search"><span aria-hidden="true">⌕</span><input id="osl-chat-search" value="${escapeHtml(model.searchQuery ?? "")}" placeholder="Search chats and messages" autocomplete="off"/></div><nav class="osl-chat-filter-tabs" aria-label="Chat filters">${tabs}</nav><div class="osl-chat-friend-list">${filter === "direct" ? friends : `<p class="osl-chat-friends-empty">No ${filter} yet.</p>`}</div><button class="osl-chat-self-row" type="button" data-osl-chat-profile>${avatar(profileName, "is-self", true)}<span><strong>${escapeHtml(profileName)}</strong><small>OSL profile</small></span>${settingsIcon}</button></aside>
    ${activeFriend ? activeThread(model, activeFriend) : emptyThread()}
  </div>`;
}
