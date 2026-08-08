export interface OslChatThreadParentMessage {
  messageId: string;
  authorName: string;
  authorId: string;
  text: string;
  timestamp: number;
}

export interface OslChatThreadReply {
  replyId: string;
  authorName: string;
  authorId: string;
  text: string;
  timestamp: number;
}

export interface OslChatThreadPaneModel {
  threadTitle: string;
  parentMessage: OslChatThreadParentMessage;
  replies: OslChatThreadReply[];
}

export function oslChatThreadPaneMarkup(model: OslChatThreadPaneModel): string {
  const formatTime = (timestamp: number): string => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString();
  };

  const escapedTitle = escapeHtml(model.threadTitle);
  const parentAuthor = escapeHtml(model.parentMessage.authorName);
  const parentText = escapeHtml(model.parentMessage.text);

  const repliesHtml = model.replies
    .map((reply) => {
      const authorName = escapeHtml(reply.authorName);
      const text = escapeHtml(reply.text);
      const time = formatTime(reply.timestamp);
      return `
        <article class="osl-chat-thread-reply" data-reply-id="${escapeHtml(reply.replyId)}">
          <div class="osl-chat-reply-header">
            <span class="osl-chat-reply-author">${authorName}</span>
            <span class="osl-chat-reply-time">${time}</span>
          </div>
          <div class="osl-chat-reply-body">${text}</div>
        </article>
      `;
    })
    .join("");

  const parentTime = formatTime(model.parentMessage.timestamp);

  return `
    <div class="osl-chat-thread-pane">
      <header class="osl-chat-thread-header">
        <h2 class="osl-chat-thread-title">${escapedTitle}</h2>
        <div class="osl-chat-thread-parent-info">
          <span class="osl-chat-parent-author">${parentAuthor}</span>
        </div>
      </header>

      <article class="osl-chat-thread-parent" data-message-id="${escapeHtml(model.parentMessage.messageId)}">
        <div class="osl-chat-parent-header">
          <span class="osl-chat-parent-author">${parentAuthor}</span>
          <span class="osl-chat-parent-time">${parentTime}</span>
        </div>
        <div class="osl-chat-parent-body">${parentText}</div>
      </article>

      <div class="osl-chat-thread-replies">
        ${repliesHtml}
      </div>

      <form class="osl-chat-thread-reply-box" data-osl-thread-reply="${escapeHtml(model.parentMessage.messageId)}">
        <textarea id="osl-thread-reply-draft" class="osl-thread-reply-textarea" placeholder="Reply to thread..."></textarea>
        <button type="submit" class="osl-thread-reply-send" disabled>Send</button>
      </form>
    </div>
  `;
}

function escapeHtml(text: string): string {
  const map: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;",
  };
  return text.replace(/[&<>"']/g, (char) => map[char] || char);
}

function avatar(value: string): string {
  return `<span class="osl-chat-avatar osl-chat-thread-pane-avatar" aria-hidden="true">${escapeHtml(initials(value))}</span>`;
}

function initials(value: string): string {
  return value.split(/\s+/u).filter(Boolean).map((part) => part[0]).join("").slice(0, 2).toUpperCase() || "?";
}

function parentRow(parent: OslChatThreadParentMessage): string {
  return `<article class="osl-chat-thread-parent" data-message-id="${escapeHtml(parent.messageId)}">
    <div class="osl-chat-thread-parent-meta"><strong>${escapeHtml(parent.author)}</strong><time>${escapeHtml(parent.timestampLabel)}</time></div>
    <p class="osl-chat-thread-parent-text">${escapeHtml(parent.body)}</p>
  </article>`;
}

function replyBox(model: OslChatThreadPaneModel): string {
  const hasDraft = model.replyDraft.trim().length > 0;
  const canSend = hasDraft && !model.busy;
  return `<form class="osl-chat-thread-reply-box" data-osl-thread-reply="${escapeHtml(model.parent.messageId)}">
    <label for="osl-thread-reply-draft">Reply in thread</label>
    <div class="osl-chat-thread-reply-bar"><textarea id="osl-thread-reply-draft" rows="1" placeholder="Reply in thread" autocomplete="off" spellcheck="true">${escapeHtml(model.replyDraft)}</textarea><button class="osl-chat-thread-reply-send" type="submit" aria-label="${model.busy ? "Sending" : "Send reply"}" ${canSend ? "" : "disabled"}>${sendIcon}<span>${model.busy ? "Sending…" : "Send"}</span></button></div>
  </form>`;
}

function replyRow(reply: OslChatThreadReply): string {
  return `<article class="osl-chat-thread-reply" data-reply-id="${escapeHtml(reply.replyId)}">
    <div class="osl-chat-thread-reply-meta"><strong>${escapeHtml(reply.author)}</strong><time>${escapeHtml(reply.timestampLabel)}</time></div>
    <p class="osl-chat-thread-reply-text">${escapeHtml(reply.body)}</p>
  </article>`;
}

const sendIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>';
