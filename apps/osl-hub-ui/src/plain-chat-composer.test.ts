import { describe, expect, it } from "vitest";
import {
  PLAIN_CHAT_COMPOSER_TITLE,
  cancelPlainChatEdit,
  cancelPlainChatReply,
  emptyPlainChatComposerModel,
  plainChatComposerEmptyStateMarkup,
  plainChatComposerMarkup,
  setPlainChatDraft,
  startPlainChatEdit,
  startPlainChatReply,
} from "./plain-chat-composer";

const pageText = (markup: string): string => markup.replace(/<[^>]*>/gu, " ").replace(/&times;/gu, "×").replace(/\s+/gu, " ").trim();

describe("plain chat composer", () => {
  it("shows the five named controls", () => {
    const markup = plainChatComposerMarkup(emptyPlainChatComposerModel("Ava"));
    expect(markup).toContain('aria-label="Attachments"');
    expect(markup).toContain('aria-label="Images"');
    expect(markup).toContain('aria-label="Emoji"');
    expect(markup).toContain('aria-label="Replies"');
    expect(markup).toContain('aria-label="Edits"');
    expect(pageText(markup)).toContain(PLAIN_CHAT_COMPOSER_TITLE);
  });

  it("never renders a lock or a pen icon", () => {
    const withReply = startPlainChatReply(emptyPlainChatComposerModel("Ava"), { authorName: "Ben", excerpt: "hello" });
    const withEdit = startPlainChatEdit(emptyPlainChatComposerModel("Ava"), { messageId: "m1", originalText: "hi" });
    for (const markup of [plainChatComposerMarkup(emptyPlainChatComposerModel("Ava")), plainChatComposerMarkup(withReply), plainChatComposerMarkup(withEdit)]) {
      expect(markup.toLowerCase()).not.toContain("lock");
      // The rect+circle+checkmark-swoosh shape used for "view once" elsewhere in
      // this app (osl-chats-view.ts `onceIcon`) is the padlock-adjacent shape
      // this task forbids; make sure this screen never reaches for it.
      expect(markup).not.toContain('<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="9" r="2"/>');
      expect(markup).not.toMatch(/aria-label="[^"]*pen[^"]*"/iu);
      expect(markup).not.toMatch(/class="[^"]*pen[^"]*"/iu);
      // The Edits control has no <svg> at all, so no pencil-nib path can hide in it.
      expect(markup).toContain('<button class="plain-composer-control plain-composer-edits');
      expect(markup).not.toMatch(/id="plain-composer-edits"[^>]*>\s*<svg/u);
    }
  });

  it("shows a reply banner once a reply starts, and clears it on cancel", () => {
    const replying = startPlainChatReply(emptyPlainChatComposerModel("Ava"), { authorName: "Ben", excerpt: "see you then" });
    const markup = plainChatComposerMarkup(replying);
    expect(pageText(markup)).toContain("Replying to Ben");
    expect(pageText(markup)).toContain("see you then");
    const cancelled = cancelPlainChatReply(replying);
    expect(pageText(plainChatComposerMarkup(cancelled))).not.toContain("Replying to Ben");
  });

  it("loads the original text into the box when an edit starts, and clears it on cancel", () => {
    const editing = startPlainChatEdit(emptyPlainChatComposerModel("Ava"), { messageId: "m1", originalText: "typo texxt" });
    expect(editing.draft).toBe("typo texxt");
    const markup = plainChatComposerMarkup(editing);
    expect(pageText(markup)).toContain("Editing message");
    expect(pageText(markup)).toContain("typo texxt");
    const cancelled = cancelPlainChatEdit(editing);
    expect(cancelled.draft).toBe("");
    expect(pageText(plainChatComposerMarkup(cancelled))).not.toContain("Editing message");
  });

  it("starting a reply cancels an edit in progress, and vice versa", () => {
    const editing = startPlainChatEdit(emptyPlainChatComposerModel("Ava"), { messageId: "m1", originalText: "hi" });
    const thenReplying = startPlainChatReply(editing, { authorName: "Cy", excerpt: "ok" });
    expect(thenReplying.editTarget).toBeNull();
    expect(thenReplying.replyTarget).not.toBeNull();

    const replying = startPlainChatReply(emptyPlainChatComposerModel("Ava"), { authorName: "Cy", excerpt: "ok" });
    const thenEditing = startPlainChatEdit(replying, { messageId: "m2", originalText: "hey" });
    expect(thenEditing.replyTarget).toBeNull();
    expect(thenEditing.editTarget).not.toBeNull();
  });

  it("keeps the typed draft on the box", () => {
    const drafted = setPlainChatDraft(emptyPlainChatComposerModel("Ava"), "on my way");
    expect(pageText(plainChatComposerMarkup(drafted))).toContain("on my way");
  });

  it("differs from the empty state it replaces", () => {
    const empty = plainChatComposerEmptyStateMarkup();
    const built = plainChatComposerMarkup(emptyPlainChatComposerModel("Ava"));
    expect(empty).not.toEqual(built);
    for (const name of ["Attachments", "Images", "Emoji", "Replies", "Edits"]) {
      expect(empty.includes(`aria-label="${name}"`)).toBe(false);
    }
  });
});
