import { describe, expect, it } from "vitest";
import {
  acceptDroppedFilesIntoTray,
  attachmentCardMarkup,
  attachmentTrayMarkup,
  createOslChatAttachmentTray,
  type OslChatDroppedFileFixture,
} from "./chat-attachment-drop";

const FIXTURE_DROP: OslChatDroppedFileFixture[] = [
  { name: "task-1336-alpha.txt", size: 5 },
  { name: "task-1336-beta.png", size: 4 },
];

describe("TASK 1336 - connect drag and drop to the chat box", () => {
  it("a fixture drop shows two file cards before Send is pressed", () => {
    let sendPressed = false;
    const tray = createOslChatAttachmentTray();

    const receipt = acceptDroppedFilesIntoTray(tray, FIXTURE_DROP);

    const cards = tray.attachments.map((attachment) => attachmentCardMarkup(attachment));
    const trayMarkup = attachmentTrayMarkup(tray);
    const cardMatches = trayMarkup.match(/data-osl-chat-drop-card=/gu) ?? [];

    console.log(`TASK1336_ACCEPTED_FILE_COUNT=${receipt.acceptedFileCount}`);
    console.log(`TASK1336_TRAY_FILE_COUNT=${receipt.trayFileCount}`);
    console.log(`TASK1336_MESSAGES_CREATED=${receipt.messagesCreated}`);
    console.log(`TASK1336_CARD_COUNT=${cards.length}`);
    console.log(`TASK1336_TRAY_MARKUP_CARD_COUNT=${cardMatches.length}`);
    console.log(`TASK1336_SEND_PRESSED=${sendPressed}`);

    expect(receipt.acceptedFileCount).toBe(2);
    expect(receipt.trayFileCount).toBe(2);
    expect(receipt.acceptedFilenames).toEqual(["task-1336-alpha.txt", "task-1336-beta.png"]);
    expect(cards).toHaveLength(2);
    expect(cardMatches).toHaveLength(2);
    expect(tray.attachments).toHaveLength(2);

    // Send is never touched by this module — the fixture drop alone must not create a message.
    expect(sendPressed).toBe(false);
    expect(receipt.messagesCreated).toBe(0);
    expect(tray.messagesCreated).toBe(0);
  });

  it("rejects an empty drop rather than staging a card", () => {
    const tray = createOslChatAttachmentTray();
    expect(() => acceptDroppedFilesIntoTray(tray, [])).toThrow(
      "Drop at least one file into the OSL Chats attachment tray",
    );
    expect(tray.attachments).toHaveLength(0);
  });
});
