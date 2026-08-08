// TASK 1305 - the plain chat composer.
//
// A typing box with five controls: attachments, images, emoji, replies and
// edits. It is deliberately "plain": unlike the OSL Chat composer in
// osl-chats-view.ts, this screen carries no encryption framing at all, so it
// must not show a lock icon (a protection claim this screen makes no
// promise about) or a pen icon (the usual edit-affordance glyph the task
// explicitly forbids). The Edits control uses a text label instead of any
// pencil-shaped icon so that rule cannot be missed by accident.
//
// This is a pure render + pure state module, like new-friend-defaults.ts: no
// timers, no `invoke`, no DOM. The markup a person sees is the markup a
// screenshot test can capture without a running hub.

export const PLAIN_CHAT_COMPOSER_TITLE = "Chat composer";

export interface PlainChatReplyTarget {
  authorName: string;
  excerpt: string;
}

export interface PlainChatEditTarget {
  messageId: string;
  originalText: string;
}

export interface PlainChatComposerModel {
  draft: string;
  placeholderName: string;
  replyTarget: PlainChatReplyTarget | null;
  editTarget: PlainChatEditTarget | null;
}

export function emptyPlainChatComposerModel(placeholderName: string): PlainChatComposerModel {
  return { draft: "", placeholderName, replyTarget: null, editTarget: null };
}

export function setPlainChatDraft(model: PlainChatComposerModel, draft: string): PlainChatComposerModel {
  return { ...model, draft };
}

/** Starting a reply cancels any edit in progress: the box can only do one at a time. */
export function startPlainChatReply(model: PlainChatComposerModel, target: PlainChatReplyTarget): PlainChatComposerModel {
  return { ...model, replyTarget: target, editTarget: null };
}

export function cancelPlainChatReply(model: PlainChatComposerModel): PlainChatComposerModel {
  return { ...model, replyTarget: null };
}

/** Starting an edit cancels any reply in progress, and loads the original text into the box. */
export function startPlainChatEdit(model: PlainChatComposerModel, target: PlainChatEditTarget): PlainChatComposerModel {
  return { ...model, editTarget: target, replyTarget: null, draft: target.originalText };
}

export function cancelPlainChatEdit(model: PlainChatComposerModel): PlainChatComposerModel {
  return { ...model, editTarget: null, draft: "" };
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

// Abstract shapes only: a paperclip hook, a picture frame, a smiley face, a
// curved reply arrow. None of these is a padlock (rect + shackle) or a pen
// nib (a narrow diagonal wedge) -- the two shapes this task forbids.
const attachmentIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M16.5 6.5 8.8 14.2a3 3 0 0 0 4.2 4.2l7-7a5 5 0 0 0-7-7l-7.2 7.2a7 7 0 0 0 9.9 9.9"/></svg>';
const imageIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="m5 18 5.5-6 4 4 2.5-3 3 3"/></svg>';
const emojiIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><circle cx="9" cy="10" r="1"/><circle cx="15" cy="10" r="1"/><path d="M8 14.5c1 1.4 2.4 2 4 2s3-.6 4-2"/></svg>';
const replyIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10 8 4 13l6 5"/><path d="M4 13h9a6 6 0 0 0 6-6V6"/></svg>';

function replyBannerMarkup(target: PlainChatReplyTarget | null): string {
  if (!target) return "";
  return `<div class="plain-composer-banner plain-composer-reply-banner" id="plain-composer-reply-banner"><span>${replyIcon}<strong>Replying to ${escapeHtml(target.authorName)}</strong><small>${escapeHtml(target.excerpt)}</small></span><button class="plain-composer-banner-cancel" id="plain-composer-cancel-reply" type="button" aria-label="Cancel reply">&times;</button></div>`;
}

function editBannerMarkup(target: PlainChatEditTarget | null): string {
  if (!target) return "";
  return `<div class="plain-composer-banner plain-composer-edit-banner" id="plain-composer-edit-banner"><span><strong>Editing message</strong><small>${escapeHtml(target.originalText)}</small></span><button class="plain-composer-banner-cancel" id="plain-composer-cancel-edit" type="button" aria-label="Cancel edit">&times;</button></div>`;
}

export function plainChatComposerMarkup(model: PlainChatComposerModel): string {
  return `<form class="plain-chat-composer" id="plain-chat-composer" aria-label="${PLAIN_CHAT_COMPOSER_TITLE}" data-plain-chat-composer>
  <h2 class="sr-only">${PLAIN_CHAT_COMPOSER_TITLE}</h2>
  ${replyBannerMarkup(model.replyTarget)}
  ${editBannerMarkup(model.editTarget)}
  <div class="plain-composer-row">
    <label class="sr-only" for="plain-composer-draft">Message</label>
    <textarea id="plain-composer-draft" class="plain-composer-draft" rows="1" placeholder="Message ${escapeHtml(model.placeholderName)}" autocomplete="off" spellcheck="true">${escapeHtml(model.draft)}</textarea>
    <div class="plain-composer-toolbar" role="toolbar" aria-label="Message controls">
      <button class="plain-composer-control" id="plain-composer-attachments" type="button" aria-label="Attachments" title="Attachments">${attachmentIcon}</button>
      <button class="plain-composer-control" id="plain-composer-images" type="button" aria-label="Images" title="Images">${imageIcon}</button>
      <button class="plain-composer-control" id="plain-composer-emoji" type="button" aria-label="Emoji" title="Emoji">${emojiIcon}</button>
      <button class="plain-composer-control${model.replyTarget ? " is-active" : ""}" id="plain-composer-replies" type="button" aria-label="Replies" title="Replies" aria-pressed="${model.replyTarget ? "true" : "false"}">${replyIcon}</button>
      <button class="plain-composer-control plain-composer-edits${model.editTarget ? " is-active" : ""}" id="plain-composer-edits" type="button" aria-label="Edits" title="Edits" aria-pressed="${model.editTarget ? "true" : "false"}"><span>Edit</span></button>
    </div>
    <button class="plain-composer-send" id="plain-composer-send" type="submit" aria-label="Send">Send</button>
  </div>
</form>`;
}

/**
 * The typing box as it stood before this task: a plain textarea and a Send
 * button, none of the five controls. Kept so the screenshot evidence can show
 * the empty state and the built composer side by side and prove the capture
 * is of something that was actually built.
 */
export function plainChatComposerEmptyStateMarkup(): string {
  return `<form class="plain-chat-composer plain-chat-composer-empty" aria-label="${PLAIN_CHAT_COMPOSER_TITLE}" data-plain-chat-composer-empty>
  <div class="plain-composer-row">
    <label class="sr-only" for="plain-composer-draft-empty">Message</label>
    <textarea id="plain-composer-draft-empty" class="plain-composer-draft" rows="1" placeholder="Message"></textarea>
    <button class="plain-composer-send" type="submit" aria-label="Send">Send</button>
  </div>
</form>`;
}
