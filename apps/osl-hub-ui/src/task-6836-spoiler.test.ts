import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  OSL_CHAT_SPOILER_ATTACHMENT_PREFIX,
  OSL_CHAT_SPOILER_NOTIFICATION,
  OSL_CHAT_SPOILER_TEXT_PREFIX,
  concealedSpoilerAttachmentControlMarkup,
  formatOslChatAttachmentFilename,
  formatOslChatText,
  loadSpoilerReveals,
  parseOslChatAttachmentFilename,
  parseOslChatText,
  persistSpoilerReveal,
  projectSpoilerContent,
  projectSpoilerAttachment,
  spoilerRevealId,
  spoilerRevealStorageKey,
  type SpoilerLocalStore,
} from "./osl-chat-spoilers";
import { oslChatsViewMarkup, type OslChatsViewModel } from "./osl-chats-view";

const SECRET_TEXT = "the launch phrase is cobalt hummingbird";
const SECRET_FILE = "quarterly-acquisition-plan.pdf";
const MESSAGE_ID = "peer-11111111111111111111111111111111";
const ATTACHMENT_ID = "peer-22222222222222222222222222222222";

class MemorySecureStore implements SpoilerLocalStore {
  readonly sealed = new Map<string, string>();
  writes = 0;

  async getItem(key: string): Promise<string | null> {
    return this.sealed.get(key) ?? null;
  }

  async setItem(key: string, value: string): Promise<void> {
    this.writes += 1;
    this.sealed.set(key, value);
  }
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function model(revealedSpoilerIds: ReadonlySet<string>): OslChatsViewModel {
  return {
    friends: [{
      personId: "sender-member",
      nickname: "Sender",
      verified: true,
      ready: true,
      preview: "SPOILER",
      previewVisible: true,
      unreadCount: 1,
      handshakeConfirmed: true,
    }],
    activePersonId: "sender-member",
    messages: [{
      messageId: MESSAGE_ID,
      direction: "incoming",
      body: SECRET_TEXT,
      format: "spoiler",
      state: "received",
      timestampLabel: "9:41 PM",
    }],
    draft: "",
    busy: false,
    spoiler: true,
    revealedSpoilerIds,
  };
}

describe("TASK 6836 encrypted SPOILER messages and reveals", () => {
  it("formats text and attachment metadata inside the already encrypted logical content", () => {
    const encryptedTextPlaintext = formatOslChatText(SECRET_TEXT, "spoiler");
    expect(encryptedTextPlaintext).toBe(`${OSL_CHAT_SPOILER_TEXT_PREFIX}${SECRET_TEXT}`);
    expect(parseOslChatText(encryptedTextPlaintext)).toEqual({ format: "spoiler", body: SECRET_TEXT });

    const encryptedAttachmentFilename = formatOslChatAttachmentFilename(SECRET_FILE, "spoiler");
    expect(encryptedAttachmentFilename).toBe(`${OSL_CHAT_SPOILER_ATTACHMENT_PREFIX}${SECRET_FILE}`);
    expect(encryptedAttachmentFilename.endsWith(".pdf")).toBe(true);
    expect(parseOslChatAttachmentFilename(encryptedAttachmentFilename)).toEqual({ format: "spoiler", body: SECRET_FILE });
  });

  it("gives two recipients an accessible control with zero secret content, then persists one local reveal only", async () => {
    const memberAStore = new MemorySecureStore();
    const memberBStore = new MemorySecureStore();
    const memberA = "recipient-a";
    const memberB = "recipient-b";
    const revealId = spoilerRevealId("message", MESSAGE_ID);
    const encodedAttachment = formatOslChatAttachmentFilename(SECRET_FILE, "spoiler");

    const initialA = oslChatsViewMarkup(model(await loadSpoilerReveals(memberAStore, memberA)));
    const initialB = oslChatsViewMarkup(model(await loadSpoilerReveals(memberBStore, memberB)));
    let textControls = 0;
    for (const initial of [initialA, initialB]) {
      expect(initial).toContain('data-osl-chat-spoiler-reveal="message:peer-11111111111111111111111111111111"');
      expect(initial).toContain('aria-label="Reveal spoiler from Sender"');
      expect(occurrences(initial, SECRET_TEXT), "screen/accessibility secret content before reveal").toBe(0);
      textControls += occurrences(initial, 'aria-label="Reveal spoiler from Sender"');
    }
    expect(textControls).toBe(2);
    const concealedAttachment = projectSpoilerAttachment(encodedAttachment, "4096 bytes", false);
    expect(concealedAttachment.filename, "attachment filename before reveal").toBe("");
    expect(concealedAttachment.sizeText, "attachment size before reveal").toBe("");
    const attachmentControls = [memberA, memberB].map(() => concealedSpoilerAttachmentControlMarkup(
      spoilerRevealId("attachment", ATTACHMENT_ID),
    ));
    expect(attachmentControls).toHaveLength(2);
    for (const control of attachmentControls) {
      expect(control).toContain('aria-label="Reveal spoiler attachment"');
      expect(control, "attachment control markup filename").not.toContain(SECRET_FILE);
      expect(control, "attachment control markup size").not.toContain("4096");
    }

    const concealed = projectSpoilerContent(SECRET_TEXT, false);
    expect(concealed.screenText, "screen pixels").toBe("");
    expect(concealed.accessibilityText, "accessibility text").toBe("");
    expect(concealed.copyText, "copy").toBe("");
    expect(concealed.searchText, "search").toBe("");
    expect(concealed.previewText, "preview").toBe("");
    expect(concealed.notificationText, "notification").toBe(OSL_CHAT_SPOILER_NOTIFICATION);
    expect(concealed.notificationText, "notification body text").not.toContain(SECRET_TEXT);

    const revealedA = await persistSpoilerReveal(memberAStore, memberA, revealId, new Set());
    expect(memberAStore.writes, "member A local reveal write").toBe(1);
    expect(memberBStore.writes, "member B store writes after member A reveal").toBe(0);
    expect(
      spoilerRevealStorageKey(memberA),
      "member reveal storage isolation",
    ).not.toBe(spoilerRevealStorageKey(memberB));

    // Restart: reconstruct state only from each member's own encrypted local store.
    const restartedA = await loadSpoilerReveals(memberAStore, memberA);
    const restartedB = await loadSpoilerReveals(memberBStore, memberB);
    expect(restartedA, "member A reveal state after restart").toEqual(revealedA);
    expect(restartedB.size, "member B reveal state after member A reveal").toBe(0);
    expect(occurrences(oslChatsViewMarkup(model(restartedA)), SECRET_TEXT), "member A after restart").toBe(1);
    expect(occurrences(oslChatsViewMarkup(model(restartedB)), SECRET_TEXT), "member B after A reveal").toBe(0);

    const attachmentRevealId = spoilerRevealId("attachment", ATTACHMENT_ID);
    const withAttachment = await persistSpoilerReveal(memberAStore, memberA, attachmentRevealId, restartedA);
    expect(withAttachment.has(attachmentRevealId)).toBe(true);
    expect(projectSpoilerAttachment(encodedAttachment, "4096 bytes", true)).toMatchObject({
      filename: SECRET_FILE,
      sizeText: "4096 bytes",
      concealed: false,
    });

    // There is no read-event dependency in persistSpoilerReveal; its only
    // observable side effect is the member-local store write above.
    console.log(`TASK6836 recipients=${[initialA, initialB].length} initial_screen_secret_pixels=${occurrences(initialA + initialB, SECRET_TEXT)} initial_accessibility_secret_text=${concealed.accessibilityText.length} initial_notification_secret_text=${occurrences(concealed.notificationText, SECRET_TEXT)} text_controls=${textControls} attachment_controls=${attachmentControls.length} member_a_restart_revealed=${Number(restartedA.has(revealId))} member_b_revealed=${restartedB.size} read_events=0 copy_before=${concealed.copyText.length} search_before=${concealed.searchText.length} preview_before=${concealed.previewText.length} text_after=${occurrences(oslChatsViewMarkup(model(restartedA)), SECRET_TEXT)} attachment_after=${Number(projectSpoilerAttachment(encodedAttachment, "4096 bytes", true).filename === SECRET_FILE)}`);
  });

  it("wires the composer, encrypted receive paths, attachment picker, local reveal, and structural CSS concealment", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const runtime = readFileSync(new URL("./osl-chat-runtime.ts", import.meta.url), "utf8");
    const view = readFileSync(new URL("./osl-chats-view.ts", import.meta.url), "utf8");
    const adapter = readFileSync(new URL("./native-overlay-adapter.ts", import.meta.url), "utf8");
    const rustMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const rustAttachment = readFileSync(new URL("../../osl-hub/src/native_attachment_transport.rs", import.meta.url), "utf8");
    const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

    expect(view).toContain('id="osl-chat-spoiler"');
    expect(view).toContain("SPOILER");
    expect(view).not.toContain("filter: blur");
    expect(main).toContain("formatOslChatText(draft, format)");
    expect(main).toContain("selectOslChatAttachment(oslChatViewOnce, oslChatSpoiler)");
    expect(main).toContain("persistSpoilerReveal(oslChatSecureStore, personId, revealId");
    expect(main).not.toMatch(/persistSpoilerReveal[^;]+(?:openOslChatText|acknowledg|readEvent)/su);
    expect(runtime.match(/parseOslChatText\(/gu)?.length).toBe(2);
    expect(adapter).toContain('{ viewOnce, spoiler }');
    expect(rustMain).toContain("view_once: bool,\n    spoiler: bool,");
    expect(rustAttachment).toContain('OSL_CHAT_SPOILER_FILENAME_PREFIX: &str = "OSL-SPOILER-1--"');

    const spoilerCss = css.match(/\.osl-chat-(?:spoiler|message-text\.is-spoiler)[\s\S]{0,900}/gu)?.join("\n") ?? "";
    expect(spoilerCss).not.toMatch(/(?:filter\s*:\s*blur|backdrop-filter)/u);
    expect(css).toContain(".osl-chat-spoiler-control");
  });
});
