import type { OslChatMessage } from "./osl-chats-view";

export interface OslChatThreadParentMessage {
  messageId: string;
  author: string;
  body: string;
  timestampLabel: string;
}

export interface OslChatThreadReply {
  replyId: string;
  author: string;
  body: string;
  timestampLabel: string;
}

export interface OslChatThreadPaneModel {
  parent: OslChatThreadParentMessage;
  replies: readonly OslChatThreadReply[];
  replyDraft: string;
  busy: boolean;
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

function initials(value: string): string {
  return value.split(/\s+/u).filter(Boolean).map((part) => part[0]).join("").slice(0, 2).toUpperCase() || "?";
}

function avatar(value: string): string {
  return `<span class="osl-chat-avatar osl-chat-thread-pane-avatar" aria-hidden="true">${escapeHtml(initials(value))}</span>`;
}

const sendIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>';

function replyBox(model: OslChatThreadPaneModel): string {
  const hasDraft = model.replyDraft.trim().length > 0;
  const canSend = hasDraft && !model.busy;
  return `<form class="osl-chat-thread-reply-box" data-osl-thread-reply="${escapeHtml(model.parent.messageId)}">
    <label for="osl-thread-reply-draft">Reply in thread</label>
    <div class="osl-chat-thread-reply-bar"><textarea id="osl-thread-reply-draft" rows="1" placeholder="Reply in thread" autocomplete="off" spellcheck="true">${escapeHtml(model.replyDraft)}</textarea><button class="osl-chat-thread-reply-send" type="submit" aria-label="${model.busy ? "Sending" : "Send reply"}" ${canSend ? "" : "disabled"}>${sendIcon}<span>${model.busy ? "Sending…" : "Send"}</span></button></div>
  </form>`;
}

function parentRow(parent: OslChatThreadParentMessage): string {
  return `<article class="osl-chat-thread-parent" data-message-id="${escapeHtml(parent.messageId)}">
    <div class="osl-chat-thread-parent-meta"><strong>${escapeHtml(parent.author)}</strong><time>${escapeHtml(parent.timestampLabel)}</time></div>
    <p class="osl-chat-thread-parent-text">${escapeHtml(parent.body)}</p>
  </article>`;
}

function replyRow(reply: OslChatThreadReply): string {
  return `<article class="osl-chat-thread-reply" data-reply-id="${escapeHtml(reply.replyId)}">
    <div class="osl-chat-thread-reply-meta"><strong>${escapeHtml(reply.author)}</strong><time>${escapeHtml(reply.timestampLabel)}</time></div>
    <p class="osl-chat-thread-reply-text">${escapeHtml(reply.body)}</p>
  </article>`;
}

export function oslChatThreadPaneMarkup(model: OslChatThreadPaneModel): string {
  const replies = model.replies.length
    ? model.replies.map((reply) => replyRow(reply)).join("")
    : '<p class="osl-chat-thread-replies-empty">No replies yet.</p>';
  return `<section class="osl-chat-thread-pane" aria-label="Thread replies">
    <header class="osl-chat-thread-pane-header">${avatar(model.parent.author)}<div><h2>Thread</h2><span>${escapeHtml(model.parent.author)}</span></div></header>
    <div class="osl-chat-thread-pane-body">
      ${parentRow(model.parent)}
      <div class="osl-chat-thread-replies" role="log" aria-live="polite" aria-relevant="additions text">${replies}</div>
    </div>
    ${replyBox(model)}
  </section>`;
}
