import type { VerificationWarningSurface } from "./verification-warning";

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
}

export interface OslChatMessage {
  messageId: string;
  direction: OslChatMessageDirection;
  body: string;
  state: OslChatDeliveryState;
  timestampLabel: string;
}

export interface OslChatsViewModel {
  friends: readonly OslChatFriend[];
  activePersonId: string | null;
  messages: readonly OslChatMessage[];
  draft: string;
  busy: boolean;
  viewOnce?: boolean;
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
  verificationWarningSurface?: VerificationWarningSurface;
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

const chatIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17.5 3.5 20v-5.2A8 8 0 0 1 3 12c0-4.4 4-8 9-8s9 3.6 9 8-4 8-9 8a10 10 0 0 1-5-1.5Z"/></svg>';
const settingsIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></svg>';
const onceIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="9" r="2"/><path d="m4 17 5-5 4 4 2-2 5 4"/></svg>';
const sendIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>';

function initials(value: string): string {
  return value.split(/\s+/u).filter(Boolean).map((part) => part[0]).join("").slice(0, 2).toUpperCase() || "?";
}

function avatar(value: string, className = ""): string {
  return `<span class="osl-chat-avatar ${className}" aria-hidden="true">${escapeHtml(initials(value))}</span>`;
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
  return `<div class="osl-chat-friend${active ? " is-active" : ""}" data-person-id="${escapeHtml(friend.personId)}">
    <button class="osl-chat-friend-open" type="button" data-osl-chat-open="${escapeHtml(friend.personId)}" ${active ? 'aria-current="true"' : ""} ${busy || !friend.verified ? 'disabled aria-disabled="true"' : ""}>
      ${avatar(friend.nickname)}<span class="osl-chat-friend-copy"><strong>${escapeHtml(friend.nickname)}</strong>${friendPreview(friend)}</span><span class="osl-chat-kind" title="Direct message">${chatIcon}</span>
    </button>
    <button class="osl-chat-friend-settings" type="button" data-osl-chat-settings="${escapeHtml(friend.personId)}" aria-label="Settings for ${escapeHtml(friend.nickname)}">${settingsIcon}</button>
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

function messageRow(message: OslChatMessage, friend: OslChatFriend): string {
  const label = deliveryLabel(message.state);
  const unreadable = oslChatMessageUnreadableNote(message, friend.handshakeConfirmed === true);
  return `<article class="osl-chat-message is-${message.direction}" data-message-id="${escapeHtml(message.messageId)}">
    <div class="osl-chat-message-meta"><strong>${message.direction === "outgoing" ? "You" : escapeHtml(friend.nickname)}</strong><time>${escapeHtml(message.timestampLabel)}</time></div><p class="osl-chat-message-text">${escapeHtml(message.body)}</p>
    <footer><span class="osl-chat-message-state is-${message.state}">${label}</span>${unreadable ? `<span class="osl-chat-message-unreadable">${escapeHtml(unreadable)}</span>` : ""}</footer>
  </article>`;
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

function emptyThread(): string {
  return `<section class="osl-chat-thread is-empty" aria-label="OSL direct chat">
    <p>Select a friend.</p>
  </section>`;
}

function activeThread(model: OslChatsViewModel, friend: OslChatFriend): string {
  const bytes = oslChatDraftBytes(model.draft);
  const withinLimit = bytes <= OSL_CHAT_MAX_DRAFT_BYTES;
  const hasDraft = model.draft.trim().length > 0;
  const mutualReady = oslChatMutualReadiness(friend);
  const peerState = mutualReady ? "mutual" : "one-way";
  const canSend = mutualReady && hasDraft && withinLimit && !model.busy;
  const readiness = !friend.verified
    ? "Verify this friend to chat."
    : !friend.ready
      ? "Chat is not ready."
      : !mutualReady
        ? "Chat needs both people to answer before sending."
      : "";
  const messages = model.messages.length
    ? model.messages.map((message) => messageRow(message, friend)).join("")
    : '<p class="osl-chat-thread-empty">No messages yet.</p>';
  const warningSurface = model.verificationWarningSurface ?? "conversation-open";
  const handshakeWarning = warningSurface === "none" ? "" : oslChatHandshakeWarning(friend);
  const unconfirmed = handshakeWarning
    ? `<p class="osl-chat-handshake-warning is-${warningSurface}" role="status" data-verification-warning-surface="${warningSurface}">${escapeHtml(handshakeWarning)}</p>`
    : "";
  return `<section class="osl-chat-thread" aria-label="OSL direct chat with ${escapeHtml(friend.nickname)}">
    ${unconfirmed}
    <header class="osl-chat-thread-header">${avatar(friend.nickname, "is-thread")}<div><h2>${escapeHtml(friend.nickname)}</h2><span>${friend.ready ? "Ready" : "Connecting"} · ${friend.verified ? "Verified" : "Unverified"}</span></div><button class="osl-chat-thread-settings" type="button" data-osl-chat-settings="${escapeHtml(friend.personId)}" aria-label="Chat settings">${settingsIcon}</button></header>
    <div class="osl-chat-message-list" role="log" aria-live="polite" aria-relevant="additions text">${messages}</div>
    ${buildIntegrityWarningRow(model.buildIntegrity)}
    ${deletionUnconfirmedRow(model.deletionUnconfirmed ?? 0)}
    <form class="osl-chat-composer" data-osl-chat-compose="${escapeHtml(friend.personId)}">
      <label for="osl-chat-draft">Message</label>
      <div class="osl-chat-composer-bar"><label class="osl-chat-view-once" title="View once"><input id="osl-chat-view-once" type="checkbox" ${model.viewOnce ? "checked" : ""} ${model.busy ? "disabled" : ""}/>${onceIcon}<span><strong>View once</strong><small>Kept out of OSL history. OSL asks for the sent copy to be deleted once it is opened, and says here when it cannot confirm that.</small></span></label><textarea id="osl-chat-draft" rows="1" placeholder="Message ${escapeHtml(friend.nickname)}" autocomplete="off" spellcheck="true" aria-describedby="osl-chat-draft-count osl-chat-readiness">${escapeHtml(model.draft)}</textarea><button class="osl-chat-send" type="submit" aria-label="${model.busy ? "Sending" : "Send"}" data-osl-chat-peer-state="${peerState}" data-osl-chat-send-context="${mutualReady && !model.busy ? "1" : "0"}" ${canSend ? "" : "disabled"}>${sendIcon}<span>${model.busy ? "Sending…" : "Send"}</span></button></div>
      <div class="osl-chat-composer-meta"><span id="osl-chat-readiness" class="osl-chat-readiness">${readiness}</span><output id="osl-chat-draft-count" class="osl-chat-byte-count${withinLimit ? "" : " is-over"}">${bytes.toLocaleString("en-US")} / ${OSL_CHAT_MAX_DRAFT_BYTES.toLocaleString("en-US")}</output></div>
    </form>
  </section>`;
}

export function oslChatsViewMarkup(model: OslChatsViewModel): string {
  const activeFriend = model.activePersonId
    ? model.friends.find((friend) => friend.personId === model.activePersonId) ?? null
    : null;
  const friends = model.friends.length
    ? model.friends.map((friend) => friendRow(friend, model.activePersonId, model.busy)).join("")
    : '<p class="osl-chat-friends-empty">No friends yet.</p>';
  const home = model.homeLogoUrl
    ? `<button class="osl-chat-home" data-route="home" type="button" aria-label="OSL Home" title="OSL Home"><img src="${escapeHtml(model.homeLogoUrl)}" alt=""/></button>`
    : "";
  return `<div class="osl-chats-view">
    <aside class="osl-chat-friends" aria-label="Direct messages"><header>${home}<span class="osl-chat-type is-active" title="Direct messages">${chatIcon}</span></header><div class="osl-chat-friend-list">${friends}</div></aside>
    ${activeFriend ? activeThread(model, activeFriend) : emptyThread()}
  </div>`;
}
