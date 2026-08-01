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
  id: "osl-chat" | "osl-circles" | "osl-mail";
  label: "OSL Chat" | "OSL Circles" | "OSL Mail";
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
    id: "osl-circles",
    label: "OSL Circles",
    destination: "Inbox",
    state: "coming_later",
    primaryAction: "Create a circle",
    protectionScope: "OSL-owned groups",
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

function deliveryLabel(state: OslChatDeliveryState): string {
  switch (state) {
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

function messageRow(message: OslChatMessage, friend: OslChatFriend): string {
  const label = deliveryLabel(message.state);
  const unreadable = oslChatMessageUnreadableNote(message, friend.handshakeConfirmed === true);
  return `<article class="osl-chat-message is-${message.direction}" data-message-id="${escapeHtml(message.messageId)}">
    <div class="osl-chat-message-meta"><strong>${message.direction === "outgoing" ? "You" : escapeHtml(friend.nickname)}</strong><time>${escapeHtml(message.timestampLabel)}</time></div><p class="osl-chat-message-text">${escapeHtml(message.body)}</p>
    <footer><span class="osl-chat-message-state is-${message.state}">${label}</span>${unreadable ? `<span class="osl-chat-message-unreadable">${escapeHtml(unreadable)}</span>` : ""}</footer>
  </article>`;
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
  const canSend = friend.verified && friend.ready && hasDraft && withinLimit && !model.busy;
  const readiness = !friend.verified
    ? "Verify this friend to chat."
    : !friend.ready
      ? "Chat is not ready."
      : "";
  const messages = model.messages.length
    ? model.messages.map((message) => messageRow(message, friend)).join("")
    : '<p class="osl-chat-thread-empty">No messages yet.</p>';
  const handshakeWarning = oslChatHandshakeWarning(friend);
  const unconfirmed = handshakeWarning
    ? `<p class="osl-chat-handshake-warning" role="status">${escapeHtml(handshakeWarning)}</p>`
    : "";
  return `<section class="osl-chat-thread" aria-label="OSL direct chat with ${escapeHtml(friend.nickname)}">
    ${unconfirmed}
    <header class="osl-chat-thread-header">${avatar(friend.nickname, "is-thread")}<div><h2>${escapeHtml(friend.nickname)}</h2><span>${friend.ready ? "Ready" : "Connecting"} · ${friend.verified ? "Verified" : "Unverified"}</span></div><button class="osl-chat-thread-settings" type="button" data-osl-chat-settings="${escapeHtml(friend.personId)}" aria-label="Chat settings">${settingsIcon}</button></header>
    <div class="osl-chat-message-list" role="log" aria-live="polite" aria-relevant="additions text">${messages}</div>
    <form class="osl-chat-composer" data-osl-chat-compose="${escapeHtml(friend.personId)}">
      <label for="osl-chat-draft">Message</label>
      <div class="osl-chat-composer-bar"><label class="osl-chat-view-once" title="View once"><input id="osl-chat-view-once" type="checkbox" ${model.viewOnce ? "checked" : ""} ${model.busy ? "disabled" : ""}/>${onceIcon}<span><strong>View once</strong><small>Removed after it is opened and kept out of OSL history.</small></span></label><textarea id="osl-chat-draft" rows="1" placeholder="Message ${escapeHtml(friend.nickname)}" autocomplete="off" spellcheck="true" aria-describedby="osl-chat-draft-count osl-chat-readiness">${escapeHtml(model.draft)}</textarea><button class="osl-chat-send" type="submit" aria-label="${model.busy ? "Sending" : "Send"}" data-osl-chat-send-context="${friend.verified && friend.ready && !model.busy ? "1" : "0"}" ${canSend ? "" : "disabled"}>${sendIcon}<span>${model.busy ? "Sending…" : "Send"}</span></button></div>
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
