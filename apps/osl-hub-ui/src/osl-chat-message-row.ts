import "./osl-chat-message-row.css";

import {
  CHAT_MESSAGE_FONTS,
  loadChatMessagesPreferences,
  normaliseChatMessagesPreferences,
  type ChatMessagesPreferences,
} from "./chat-messages-pane";

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
  /** False for a continuation from the same author; omitted means a group head. */
  startsGroup?: boolean;
  replyTo?: OslChatMessageReplyReference;
}

export interface OslChatMessageRowOptions {
  /** A fixture or preview may override individual saved choices. */
  preferences?: Partial<ChatMessagesPreferences>;
  storage?: Pick<Storage, "getItem"> | null;
}

/**
 * Draw one row using the same durable preference record as the Messages pane.
 * CSS variables retain the saved numeric and colour values without a second
 * mapping layer that could round or substitute them.
 */
export function oslChatMessageRowMarkup(
  model: OslChatMessageRowModel,
  options: OslChatMessageRowOptions = {},
): string {
  const storage = options.storage === undefined ? globalThis.localStorage : options.storage;
  const preferences = normaliseChatMessagesPreferences({
    ...loadChatMessagesPreferences(storage),
    ...options.preferences,
  });
  const fontFamily = CHAT_MESSAGE_FONTS.find(({ id }) => id === preferences.font)?.family
    ?? CHAT_MESSAGE_FONTS[0].family;
  const startsGroup = model.startsGroup !== false;
  const authorName = escapeHtml(model.authorName);
  const initial = escapeHtml(Array.from(model.authorName.trim())[0]?.toUpperCase() ?? "?");
  const text = escapeHtml(model.text);
  const time = new Date(model.timestamp).toLocaleTimeString();
  const style = [
    `--osl-chat-message-font-family:${fontFamily}`,
    `--osl-chat-message-font-size:${preferences.textSize}px`,
    `--osl-chat-message-corner-rounding:${preferences.cornerRounding}px`,
    `--osl-chat-message-group-spacing:${preferences.spacing}px`,
    `--osl-chat-message-own-colour:${preferences.messageColour}`,
  ].join(";");

  const replyReferenceHtml = model.replyTo
    ? `<div class="osl-chat-message-reply-reference" data-reply-to-id="${escapeHtml(model.replyTo.messageId)}"><span class="osl-chat-reply-reference-author">${escapeHtml(model.replyTo.authorName)}</span><span class="osl-chat-reply-reference-text">${escapeHtml(model.replyTo.text)}</span></div>`
    : "";
  const headerHtml = startsGroup
    ? `<div class="osl-chat-message-header"><span class="osl-chat-message-author">${authorName}</span><time class="osl-chat-message-time" datetime="${new Date(model.timestamp).toISOString()}">${escapeHtml(time)}</time></div>`
    : "";
  const editControlHtml = model.isOwnMessage
    ? `<button type="button" class="osl-chat-message-edit" data-osl-edit-message="${escapeHtml(model.messageId)}">Edit</button>`
    : "";

  return `<article class="osl-chat-message-row" data-message-id="${escapeHtml(model.messageId)}" data-author-id="${escapeHtml(model.authorId)}" data-own-message="${model.isOwnMessage}" data-bubbles="${preferences.bubbles}" data-density="${preferences.density}" data-group-start="${startsGroup}" data-message-font="${preferences.font}" style="${escapeHtml(style)}">
    <div class="osl-chat-message-avatar" aria-hidden="true">${startsGroup ? initial : ""}</div>
    <div class="osl-chat-message-content">
      ${headerHtml}
      ${replyReferenceHtml}
      <div class="osl-chat-message-body">${text}</div>
      <div class="osl-chat-message-controls">${editControlHtml}</div>
    </div>
  </article>`;
}

function escapeHtml(text: string): string {
  const map: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;",
  };
  return text.replace(/[&<>"']/gu, (character) => map[character] ?? character);
}
