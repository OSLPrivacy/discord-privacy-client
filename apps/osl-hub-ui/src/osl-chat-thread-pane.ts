/**
 * A compact thread view which deliberately owns only its presentation.  The
 * background settings are read from the settings pane's stable storage keys so
 * this view can be mounted independently of the settings route.
 */
import "./osl-chat-thread-pane.css";

export type OslChatThreadBackground = "none" | "ink" | "slate" | "deep-teal" | "dusk" | "moss" | "ember" | "grid" | "upload";
export type OslChatThreadBackgroundScope = "chat" | "every-chat";

export interface OslChatThreadBackgroundSettings {
  background: OslChatThreadBackground;
  uploadedImage?: string;
  blur: boolean;
  motion: boolean;
  scope: OslChatThreadBackgroundScope;
}

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
  /** Conversation identity used by the "This chat only" saved setting. */
  chatId?: string;
  parentMessage: OslChatThreadParentMessage;
  replies: OslChatThreadReply[];
  /** Supplying settings is useful for server-rendered and capture fixtures. */
  background?: OslChatThreadBackgroundSettings;
}

type StorageLike = Pick<Storage, "getItem">;
type StoredByChat = Record<string, Partial<OslChatThreadBackgroundSettings>>;

const GLOBAL_KEY = "osl-chat-background-global-v1";
const BY_CHAT_KEY = "osl-chat-background-by-chat-v1";
const DEFAULT_BACKGROUND: OslChatThreadBackgroundSettings = {
  background: "none", blur: false, motion: true, scope: "every-chat",
};
const backgrounds = new Set<OslChatThreadBackground>(["none", "ink", "slate", "deep-teal", "dusk", "moss", "ember", "grid", "upload"]);

function escapeHtml(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#039;");
}

function parsed(storage: StorageLike, key: string): unknown {
  try { return JSON.parse(storage.getItem(key) ?? "null"); } catch { return null; }
}

function normalise(value: unknown, fallback = DEFAULT_BACKGROUND): OslChatThreadBackgroundSettings {
  const candidate = value && typeof value === "object" ? value as Partial<OslChatThreadBackgroundSettings> : {};
  return {
    background: backgrounds.has(candidate.background as OslChatThreadBackground) ? candidate.background as OslChatThreadBackground : fallback.background,
    uploadedImage: typeof candidate.uploadedImage === "string" ? candidate.uploadedImage : undefined,
    blur: typeof candidate.blur === "boolean" ? candidate.blur : fallback.blur,
    motion: typeof candidate.motion === "boolean" ? candidate.motion : fallback.motion,
    scope: candidate.scope === "chat" || candidate.scope === "every-chat" ? candidate.scope : fallback.scope,
  };
}

/** Read exactly the global-or-chat precedence used by the background settings pane. */
export function loadOslChatThreadBackground(
  chatId: string,
  storage: StorageLike = typeof localStorage === "undefined" ? { getItem: () => null } : localStorage,
): OslChatThreadBackgroundSettings {
  const global = normalise(parsed(storage, GLOBAL_KEY));
  const saved = parsed(storage, BY_CHAT_KEY);
  const override = saved && typeof saved === "object" ? (saved as StoredByChat)[chatId] : undefined;
  return override ? normalise(override, global) : global;
}

function backgroundStyle(settings: OslChatThreadBackgroundSettings): string {
  // Image URLs are passed as a custom property, never spliced into a CSS rule.
  // The settings pane writes data URLs; quoting also preserves ordinary URLs.
  const image = settings.background === "upload" && settings.uploadedImage
    ? `--osl-thread-background-image:url(&quot;${escapeHtml(settings.uploadedImage)}&quot;);`
    : "";
  return image;
}

export function oslChatThreadPaneMarkup(model: OslChatThreadPaneModel): string {
  const chatId = model.chatId ?? model.parentMessage.authorId;
  const settings = model.background ?? loadOslChatThreadBackground(chatId);
  const formatTime = (timestamp: number): string => new Date(timestamp).toLocaleTimeString();
  const replies = model.replies.map((reply) => `<article class="osl-chat-thread-reply" data-reply-id="${escapeHtml(reply.replyId)}"><div class="osl-chat-reply-header"><span class="osl-chat-reply-author">${escapeHtml(reply.authorName)}</span><span class="osl-chat-reply-time">${formatTime(reply.timestamp)}</span></div><div class="osl-chat-reply-body">${escapeHtml(reply.text)}</div></article>`).join("");
  const background = `data-osl-chat-background="${settings.background}" data-osl-chat-background-blur="${settings.blur}" data-osl-chat-background-motion="${settings.motion}"`;

  return `<div class="osl-chat-thread-pane" ${background} style="${backgroundStyle(settings)}">
    <header class="osl-chat-thread-header"><h2 class="osl-chat-thread-title">${escapeHtml(model.threadTitle)}</h2><div class="osl-chat-thread-parent-info"><span class="osl-chat-parent-author">${escapeHtml(model.parentMessage.authorName)}</span></div></header>
    <div class="osl-chat-thread-content">
      <article class="osl-chat-thread-parent" data-message-id="${escapeHtml(model.parentMessage.messageId)}"><div class="osl-chat-parent-header"><span class="osl-chat-parent-author">${escapeHtml(model.parentMessage.authorName)}</span><span class="osl-chat-parent-time">${formatTime(model.parentMessage.timestamp)}</span></div><div class="osl-chat-parent-body">${escapeHtml(model.parentMessage.text)}</div></article>
      <div class="osl-chat-thread-replies">${replies}</div>
    </div>
    <form class="osl-chat-thread-reply-box" data-osl-thread-reply="${escapeHtml(model.parentMessage.messageId)}"><textarea id="osl-thread-reply-draft" class="osl-thread-reply-textarea" placeholder="Reply to thread..."></textarea><button type="submit" class="osl-thread-reply-send" disabled>Send</button></form>
  </div>`;
}
