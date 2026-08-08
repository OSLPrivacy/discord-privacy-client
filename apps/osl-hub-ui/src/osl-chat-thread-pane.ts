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
