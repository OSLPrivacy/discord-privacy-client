/**
 * Presentation-only header for a selected group conversation.  Membership is
 * passed in by the caller: this component never invents a roster or a count.
 */
export interface OslGroupChatHeaderModel {
  groupId: string;
  name: string;
  memberCount: number;
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

export function oslGroupChatMemberCountLabel(memberCount: number): string {
  if (!Number.isSafeInteger(memberCount) || memberCount < 1) {
    throw new Error("A group chat must have a positive, exact member count");
  }
  return `${memberCount.toLocaleString("en-US")} ${memberCount === 1 ? "member" : "members"}`;
}

/** Renders the name, exact count, and actions for the active group chat. */
export function oslGroupChatHeaderMarkup(model: OslGroupChatHeaderModel): string {
  const name = escapeHtml(model.name);
  const groupId = escapeHtml(model.groupId);
  const memberCount = oslGroupChatMemberCountLabel(model.memberCount);
  return `<header class="osl-group-chat-header" data-osl-group-chat-id="${groupId}">
    <div class="osl-group-chat-header-copy">
      <p class="osl-group-chat-kicker">Group chat</p>
      <h1>${name}</h1>
      <p class="osl-group-chat-members" data-osl-group-member-count="${model.memberCount}">${memberCount}</p>
    </div>
    <nav class="osl-group-chat-actions" aria-label="Chat actions">
      <button type="button" class="osl-group-chat-action" data-osl-group-chat-search="${groupId}" aria-label="Search messages in ${name}">Search</button>
      <button type="button" class="osl-group-chat-action" data-osl-group-chat-settings="${groupId}" aria-label="Group settings for ${name}">Settings</button>
    </nav>
  </header>`;
}
