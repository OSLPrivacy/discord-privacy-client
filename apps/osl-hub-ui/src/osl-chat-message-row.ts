export interface OslChatMessageReplyReference {
  messageId: string;
  authorName: string;
  text: string;
}

export interface OslChatMessageRowModel {
  messageId: string;
  authorName: string;
  authorId: string;
  text: string;
  timestamp: number;
  isOwnMessage: boolean;
  replyTo?: OslChatMessageReplyReference;
}

export function oslChatMessageRowMarkup(model: OslChatMessageRowModel): string {
  const formatTime = (timestamp: number): string => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString();
  };

  const authorName = escapeHtml(model.authorName);
  const text = escapeHtml(model.text);
  const time = formatTime(model.timestamp);

  const replyReferenceHtml = model.replyTo
    ? `
        <div class="osl-chat-message-reply-reference" data-reply-to-id="${escapeHtml(model.replyTo.messageId)}">
          <span class="osl-chat-reply-reference-author">${escapeHtml(model.replyTo.authorName)}</span>
          <span class="osl-chat-reply-reference-text">${escapeHtml(model.replyTo.text)}</span>
        </div>
      `
    : "";

  const editControlHtml = model.isOwnMessage
    ? `<button type="button" class="osl-chat-message-edit" data-osl-edit-message="${escapeHtml(model.messageId)}">Edit</button>`
    : "";

  return `
    <article class="osl-chat-message-row" data-message-id="${escapeHtml(model.messageId)}" data-own-message="${model.isOwnMessage ? "true" : "false"}">
      ${replyReferenceHtml}
      <div class="osl-chat-message-header">
        <span class="osl-chat-message-author">${authorName}</span>
        <span class="osl-chat-message-time">${time}</span>
      </div>
      <div class="osl-chat-message-body">${text}</div>
      <div class="osl-chat-message-controls">
        ${editControlHtml}
      </div>
    </article>
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
