export const OSL_CHAT_MAX_DRAFT_BYTES = 1_000;

export type OslChatDeliveryState =
  | "sent"
  | "delivered"
  | "received"
  | "opened"
  | "expired"
  | "failed";

export type OslChatMessageDirection = "outgoing" | "incoming";

export interface OslChatFriend {
  personId: string;
  nickname: string;
  verified: boolean;
  ready: boolean;
  preview: string | null;
  previewVisible: boolean;
  unreadCount: number;
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
  const unread = friend.unreadCount > 0
    ? `<span class="osl-chat-unread" aria-label="${friend.unreadCount} unread">${friend.unreadCount}</span>`
    : "";
  return `<div class="osl-chat-friend${active ? " is-active" : ""}" data-person-id="${escapeHtml(friend.personId)}">
    <button class="osl-chat-friend-open" type="button" data-osl-chat-open="${escapeHtml(friend.personId)}" ${active ? 'aria-current="true"' : ""} ${busy || !friend.verified ? 'disabled aria-disabled="true"' : ""}>
      <span class="osl-chat-friend-copy"><strong>${escapeHtml(friend.nickname)}</strong>${friendPreview(friend)}</span>${unread}
    </button>
    <button class="osl-chat-friend-settings" type="button" data-osl-chat-settings="${escapeHtml(friend.personId)}" aria-label="Settings for ${escapeHtml(friend.nickname)}">Settings</button>
  </div>`;
}

function messageRow(message: OslChatMessage): string {
  const label = deliveryLabel(message.state);
  return `<article class="osl-chat-message is-${message.direction}" data-message-id="${escapeHtml(message.messageId)}">
    <p class="osl-chat-message-text">${escapeHtml(message.body)}</p>
    <footer><time>${escapeHtml(message.timestampLabel)}</time><span class="osl-chat-message-state is-${message.state}">${label}</span></footer>
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
    ? model.messages.map(messageRow).join("")
    : '<p class="osl-chat-thread-empty">No messages yet.</p>';
  return `<section class="osl-chat-thread" aria-label="OSL direct chat with ${escapeHtml(friend.nickname)}">
    <header class="osl-chat-thread-header"><div><span>OSL Chat</span><h2>${escapeHtml(friend.nickname)}</h2></div><button class="osl-chat-thread-settings" type="button" data-osl-chat-settings="${escapeHtml(friend.personId)}">Settings</button></header>
    <div class="osl-chat-message-list" role="log" aria-live="polite" aria-relevant="additions text">${messages}</div>
    <form class="osl-chat-composer" data-osl-chat-compose="${escapeHtml(friend.personId)}">
      <label for="osl-chat-draft">Message</label>
      <textarea id="osl-chat-draft" rows="3" autocomplete="off" spellcheck="true" aria-describedby="osl-chat-draft-count osl-chat-readiness">${escapeHtml(model.draft)}</textarea>
      <div class="osl-chat-composer-meta"><output id="osl-chat-draft-count" class="osl-chat-byte-count${withinLimit ? "" : " is-over"}">${bytes.toLocaleString("en-US")} / ${OSL_CHAT_MAX_DRAFT_BYTES.toLocaleString("en-US")} bytes</output><span id="osl-chat-readiness" class="osl-chat-readiness">${readiness}</span></div>
      <label class="osl-chat-view-once"><input id="osl-chat-view-once" type="checkbox" ${model.viewOnce ? "checked" : ""} ${model.busy ? "disabled" : ""}/><span><strong>View once</strong><small>Removed from the relay when opened and never added to OSL history.</small></span></label>
      <button class="osl-chat-send" type="submit" ${canSend ? "" : "disabled"}>${model.busy ? "Sending…" : "Send"}</button>
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
  return `<div class="osl-chats-view">
    <aside class="osl-chat-friends" aria-label="Friends"><header><h2>Friends</h2></header><div class="osl-chat-friend-list">${friends}</div></aside>
    ${activeFriend ? activeThread(model, activeFriend) : emptyThread()}
  </div>`;
}
