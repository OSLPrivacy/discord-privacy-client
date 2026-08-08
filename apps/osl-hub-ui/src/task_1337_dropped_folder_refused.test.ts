/**
 * TASK 1337 - check dropped folders and auto-send fail.
 *
 * Drops `maple.txt` (fingerprint `MAPLE-4172`) onto the composer tray built by
 * TASK 1336, then changes *only* that dropped item's kind from file to folder
 * and drops it again without pressing Send. The folder has to be refused
 * because folders are not allowed, and the refusal has to leave the staged card
 * exactly where it was.
 */
import { describe, expect, it } from "vitest";
import {
  OSL_CHAT_DROP_FOLDER_REFUSAL,
  acceptDroppedFilesIntoTray,
  attachmentTrayMarkup,
  createOslChatAttachmentTray,
  trayFingerprint,
  type OslChatDroppedFileFixture,
} from "./chat-attachment-drop";

/**
 * Fixture body for `maple.txt`. The intake folds the dropped bytes into the four
 * digits of the fingerprint; this body is the one that lands on `4172`, which is
 * what makes the dropped file read as `MAPLE-4172`.
 */
const MAPLE_BODY = new TextEncoder().encode("maple leaf 2176\n");
const MAPLE_FINGERPRINT = "MAPLE-4172";

const droppedMapleFile: OslChatDroppedFileFixture = {
  name: "maple.txt",
  size: MAPLE_BODY.length,
  kind: "file",
  bytes: MAPLE_BODY,
};

describe("TASK 1337 - check dropped folders and auto-send fail", () => {
  it("refuses the folder and leaves maple.txt, the tray count and the sent count alone", () => {
    let sendPressed = false;
    const tray = createOslChatAttachmentTray();

    console.log(`TASK1337_TRAY_COUNT_BEFORE=${tray.attachments.length}`);
    console.log(`TASK1337_SENT_MESSAGE_COUNT_BEFORE=${tray.messagesCreated}`);
    expect(tray.attachments).toHaveLength(0);
    expect(tray.messagesCreated).toBe(0);

    console.log(`TASK1337_DROPPED_ITEM_KIND_FIRST_DROP=${droppedMapleFile.kind}`);
    const receipt = acceptDroppedFilesIntoTray(tray, [droppedMapleFile]);

    console.log(`TASK1337_TRAY_COUNT_AFTER_FILE=${receipt.trayFileCount}`);
    console.log(`TASK1337_DROPPED_FILENAME=${receipt.acceptedFilenames.join(",")}`);
    console.log(`TASK1337_DROPPED_FINGERPRINT=${receipt.acceptedFingerprints.join(",")}`);
    console.log(`TASK1337_SENT_MESSAGE_COUNT_AFTER_FILE=${receipt.messagesCreated}`);
    console.log(`TASK1337_SEND_PRESSED_AFTER_FILE=${sendPressed}`);
    expect(receipt.acceptedFileCount).toBe(1);
    expect(receipt.trayFileCount).toBe(1);
    expect(tray.attachments).toHaveLength(1);
    expect(receipt.acceptedFilenames).toEqual(["maple.txt"]);
    expect(receipt.acceptedFingerprints).toEqual([MAPLE_FINGERPRINT]);
    expect(tray.attachments[0]?.fingerprint).toBe(MAPLE_FINGERPRINT);
    expect(receipt.messagesCreated).toBe(0);
    expect(tray.messagesCreated).toBe(0);
    expect(sendPressed).toBe(false);

    // The card the user is looking at carries the fingerprint too.
    const markupBefore = attachmentTrayMarkup(tray);
    console.log(
      `TASK1337_CARD_FINGERPRINT_BEFORE=${markupBefore.match(/data-osl-chat-drop-fingerprint="([^"]+)"/u)?.[1]}`,
    );
    expect(markupBefore).toContain(`data-osl-chat-drop-fingerprint="${MAPLE_FINGERPRINT}"`);

    // Change ONLY the dropped item's kind: same name, same bytes, same size,
    // file -> folder. Send is still never pressed.
    const droppedMapleFolder: OslChatDroppedFileFixture = { ...droppedMapleFile, kind: "folder" };
    expect({ ...droppedMapleFolder, kind: droppedMapleFile.kind }).toEqual(droppedMapleFile);
    console.log(`TASK1337_DROPPED_ITEM_KIND_SECOND_DROP=${droppedMapleFolder.kind}`);

    let refusal = "";
    try {
      acceptDroppedFilesIntoTray(tray, [droppedMapleFolder]);
    } catch (error) {
      refusal = error instanceof Error ? error.message : String(error);
    }
    console.log(`TASK1337_FOLDER_REFUSAL=${refusal}`);
    console.log(
      `TASK1337_FOLDER_REFUSAL_SAYS_FOLDERS_NOT_ALLOWED=${refusal.includes("folders are not allowed")}`,
    );
    expect(refusal).toBe(OSL_CHAT_DROP_FOLDER_REFUSAL);
    expect(refusal).toContain("folders are not allowed");

    const card = tray.attachments[0];
    console.log(`TASK1337_MAPLE_FILENAME_AFTER_FOLDER=${card?.originalFilename}`);
    console.log(`TASK1337_MAPLE_FINGERPRINT_AFTER_FOLDER=${card?.fingerprint}`);
    console.log(`TASK1337_TRAY_COUNT_AFTER_FOLDER=${tray.attachments.length}`);
    console.log(`TASK1337_SENT_MESSAGE_COUNT_AFTER_FOLDER=${tray.messagesCreated}`);
    console.log(`TASK1337_SEND_PRESSED_AFTER_FOLDER=${sendPressed}`);
    expect(card?.originalFilename).toBe("maple.txt");
    expect(card?.fingerprint).toBe(MAPLE_FINGERPRINT);
    expect(tray.attachments).toHaveLength(1);
    expect(tray.messagesCreated).toBe(0);
    expect(sendPressed).toBe(false);
    expect(attachmentTrayMarkup(tray)).toBe(markupBefore);
  });

  it("earns the fingerprint from the dropped bytes rather than storing it", () => {
    const derived = trayFingerprint("maple.txt", MAPLE_BODY);
    console.log(`TASK1337_DERIVED_FINGERPRINT=${derived}`);
    expect(derived).toBe(MAPLE_FINGERPRINT);

    const tampered = Uint8Array.from(MAPLE_BODY);
    tampered[0] = "M".charCodeAt(0);
    const afterTamper = trayFingerprint("maple.txt", tampered);
    console.log(`TASK1337_FINGERPRINT_AFTER_ONE_BYTE_CHANGE=${afterTamper}`);
    expect(afterTamper).not.toBe(MAPLE_FINGERPRINT);
  });

  it("stages nothing when a folder rides along with a file in one drop", () => {
    const tray = createOslChatAttachmentTray();
    let refusal = "";
    try {
      acceptDroppedFilesIntoTray(tray, [
        droppedMapleFile,
        { name: "maple-folder", size: 0, kind: "folder" },
      ]);
    } catch (error) {
      refusal = error instanceof Error ? error.message : String(error);
    }
    console.log(`TASK1337_MIXED_DROP_REFUSAL=${refusal}`);
    console.log(`TASK1337_MIXED_DROP_TRAY_COUNT=${tray.attachments.length}`);
    console.log(`TASK1337_MIXED_DROP_SENT_MESSAGE_COUNT=${tray.messagesCreated}`);
    expect(refusal).toBe(OSL_CHAT_DROP_FOLDER_REFUSAL);
    expect(tray.attachments).toHaveLength(0);
    expect(tray.messagesCreated).toBe(0);
  });
});
