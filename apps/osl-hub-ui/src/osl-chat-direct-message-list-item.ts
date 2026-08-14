/**
 * TASK 1310 -- the direct-message list item.
 *
 * One row of the OSL Chats direct-message list: who the conversation is with,
 * what the latest message in it was, and -- when OSL can no longer say this app
 * copy is the unmodified build it proved at startup -- the changed-build
 * warning, attached to the conversation the user is about to open.
 *
 * Three rules this file exists to keep:
 *
 * 1. The warning is the ONLY honest signal available and it is one-sided. A
 *    present {@link OslChatDirectMessageBuildWarning} means the local
 *    installed-build proof stopped matching (`installed_build.rs`
 *    `installed_build_chat_warning`); its ABSENCE means only "nothing known to
 *    be wrong", never "verified unmodified". That is why the unmodified state
 *    renders no reassuring badge at all -- it renders nothing.
 * 2. A changed build never removes chat features. `installed_build.rs` pins
 *    `message_sending_available: true` on both reasons, so this row must stay
 *    openable while warning. `oslChatDirectMessageListItemMarkup` never emits
 *    `disabled` on the open control because of a warning, and
 *    `data-message-sending-available="true"` is written from the model so a test
 *    can read it back.
 * 3. The wording is not re-invented here. {@link OSL_CHANGED_BUILD_CHAT_WARNING}
 *    is byte-for-byte `CHANGED_BUILD_CHAT_WARNING`
 *    (`apps/osl-hub/src/installed_build.rs:11`), the same string
 *    `parseInstalledBuildChatWarning` in `adapters.ts` refuses to accept any
 *    variation of.
 *
 * This module is deliberately self-contained -- no import of `osl-chats-view.ts`
 * or `adapters.ts` -- so the row can be rendered and proved on its own.
 */

/** Verbatim `CHANGED_BUILD_CHAT_WARNING` from `apps/osl-hub/src/installed_build.rs:11`. */
export const OSL_CHANGED_BUILD_CHAT_WARNING =
  "OSL build changed after its startup proof. Sending stays available.";

export type OslChatDirectMessageBuildWarningReason = "changed" | "corruptProof";

/** Same shape as `InstalledBuildChatWarning` in `adapters.ts`. */
export interface OslChatDirectMessageBuildWarning {
  kind: "changedBuild";
  reason: OslChatDirectMessageBuildWarningReason;
  message: string;
  messageSendingAvailable: true;
}

export type OslChatDirectMessageDirection = "outgoing" | "incoming";

export interface OslChatDirectMessageLatest {
  body: string;
  direction: OslChatDirectMessageDirection;
  timestampLabel: string;
}

export interface OslChatDirectMessageListItemModel {
  conversationId: string;
  /** The other person's name, as the user named them locally. */
  name: string;
  /**
   * The latest message in this conversation, or `null` when there is none.
   * `previewVisible: false` keeps the row while withholding the body -- the
   * user can turn previews off without the conversation vanishing.
   */
  latest?: OslChatDirectMessageLatest | null;
  previewVisible?: boolean;
  unreadCount?: number;
  active?: boolean;
  /** Absent or `null` means "nothing known to be wrong", NOT "verified". */
  buildWarning?: OslChatDirectMessageBuildWarning | null;
}

export interface OslChatDirectMessageListModel {
  items: readonly OslChatDirectMessageListItemModel[];
  activeConversationId?: string | null;
}

const directMessageIcon =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17.5 3.5 20v-5.2A8 8 0 0 1 3 12c0-4.4 4-8 9-8s9 3.6 9 8-4 8-9 8a10 10 0 0 1-5-1.5Z"/></svg>';

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function initials(value: string): string {
  return value.split(/\s+/u).filter(Boolean).map((part) => part[0]).join("").slice(0, 2).toUpperCase() || "?";
}

/**
 * The build state this row is allowed to state: the two warned reasons, or
 * `"unmodified"` -- which is the only value that renders no warning row.
 */
export function oslChatDirectMessageBuildState(
  warning: OslChatDirectMessageBuildWarning | null | undefined,
): "unmodified" | "changed" | "corrupt-proof" {
  if (!warning || warning.kind !== "changedBuild") return "unmodified";
  return warning.reason === "corruptProof" ? "corrupt-proof" : "changed";
}

function latestMessageCell(item: OslChatDirectMessageListItemModel): string {
  if (item.previewVisible === false) {
    return '<span class="osl-chat-dm-latest is-hidden">Preview hidden</span>';
  }
  const latest = item.latest ?? null;
  if (!latest) {
    return '<span class="osl-chat-dm-latest is-empty">No messages yet</span>';
  }
  const speaker = latest.direction === "outgoing" ? "You" : item.name;
  return `<span class="osl-chat-dm-latest" data-osl-chat-dm-latest-direction="${escapeHtml(latest.direction)}"><span class="osl-chat-dm-latest-speaker">${escapeHtml(speaker)}:</span> ${escapeHtml(latest.body)}</span>`;
}

/**
 * The changed-build warning row. Rendered only for a real warning, and it never
 * claims sending stopped -- `installed_build.rs` guarantees it did not.
 */
function buildWarningRow(warning: OslChatDirectMessageBuildWarning | null | undefined): string {
  const state = oslChatDirectMessageBuildState(warning);
  if (state === "unmodified" || !warning) return "";
  return `<p class="osl-chat-dm-build-warning warning" role="status" data-osl-chat-dm-build-warning="${state}" data-message-sending-available="${warning.messageSendingAvailable === true ? "true" : "false"}"><strong>Changed build warning</strong><small>${escapeHtml(warning.message)}</small></p>`;
}

function unreadBadge(count: number | undefined): string {
  if (!Number.isSafeInteger(count) || (count ?? 0) <= 0) return "";
  const value = count as number;
  return `<span class="osl-chat-dm-unread" data-osl-chat-dm-unread="${value}" aria-label="${value} unread">${value.toLocaleString("en-US")}</span>`;
}

export function oslChatDirectMessageListItemMarkup(
  item: OslChatDirectMessageListItemModel,
  activeConversationId: string | null = null,
): string {
  const active = item.active === true || item.conversationId === activeConversationId;
  const buildState = oslChatDirectMessageBuildState(item.buildWarning);
  const timestamp = item.previewVisible === false ? null : item.latest?.timestampLabel ?? null;
  return `<li class="osl-chat-dm-item${active ? " is-active" : ""}${buildState === "unmodified" ? "" : " has-build-warning"}" data-osl-chat-dm-item="${escapeHtml(item.conversationId)}" data-osl-chat-dm-kind="direct-message" data-osl-chat-dm-build="${buildState}">
    <button class="osl-chat-dm-open" type="button" data-osl-chat-dm-open="${escapeHtml(item.conversationId)}"${active ? ' aria-current="true"' : ""}>
      <span class="osl-chat-avatar" aria-hidden="true">${escapeHtml(initials(item.name))}</span><span class="osl-chat-dm-copy"><span class="osl-chat-dm-line"><strong class="osl-chat-dm-name">${escapeHtml(item.name)}</strong>${timestamp ? `<time class="osl-chat-dm-time">${escapeHtml(timestamp)}</time>` : ""}</span>${latestMessageCell(item)}</span><span class="osl-chat-kind" title="Direct message">${directMessageIcon}</span>${unreadBadge(item.unreadCount)}
    </button>
    ${buildWarningRow(item.buildWarning)}
  </li>`;
}

export function oslChatDirectMessageListMarkup(model: OslChatDirectMessageListModel): string {
  const activeConversationId = model.activeConversationId ?? null;
  if (!model.items.length) {
    return '<div class="osl-chat-dm-list-shell"><p class="osl-chat-dm-list-empty">No direct messages yet.</p></div>';
  }
  const rows = model.items
    .map((item) => oslChatDirectMessageListItemMarkup(item, activeConversationId))
    .join("");
  return `<div class="osl-chat-dm-list-shell"><ul class="osl-chat-dm-list" aria-label="Direct messages">${rows}</ul></div>`;
}
