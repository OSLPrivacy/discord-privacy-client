import { describe, expect, it } from "vitest";
import {
  MESSENGER_ATTACHMENT_MAX_BYTES,
  addMessengerDroppedAttachment,
  addMessengerPickerAttachment,
  createMessengerAttachmentTray,
  type MessengerAttachmentSource,
} from "./messenger-attachment-tray";

const file = (name: string, bytes: Uint8Array): MessengerAttachmentSource => ({
  name,
  size: bytes.byteLength,
  bytes,
});

describe("TASK 1186 - Messenger attachment limits", () => {
  it("refuses 17 files, an over-8-MiB file, and a folder without changing the good file", () => {
    const tray = createMessengerAttachmentTray();
    const good = file(
      "messenger-file-1186.txt",
      new TextEncoder().encode("TASK 1186 Messenger attachment boundary fixture\n"),
    );
    const receipt = addMessengerPickerAttachment(tray, [good]);
    const originalCards = tray.cards.map((card) => ({ ...card }));

    console.log(`TASK1186 good_attached_count=${receipt.trayCardCount}`);
    console.log(`TASK1186 good_attached_name=${receipt.card.name}`);
    expect(receipt.trayCardCount).toBe(1);
    expect(receipt.card.name).toBe("messenger-file-1186.txt");

    const expectRefusedWithoutMutation = (
      refusalName: "17-files" | "over-8-MB" | "folder",
      action: () => unknown,
      expectedMessage: string,
    ): void => {
      let message = "";
      try {
        action();
      } catch (error) {
        message = error instanceof Error ? error.message : String(error);
      }

      console.log(`TASK1186 refused_name=${refusalName} message=${message}`);
      expect(message, `${refusalName} must be refused`).toContain(expectedMessage);
      expect(tray.cards, `${refusalName} changed the attachment tray`).toEqual(originalCards);
    };

    const seventeenFiles = Array.from({ length: 17 }, (_, index) =>
      file(`messenger-file-${String(index + 1).padStart(2, "0")}.txt`, new Uint8Array([index])),
    );
    console.log(`TASK1186 attempted_17_files_count=${seventeenFiles.length}`);
    expectRefusedWithoutMutation(
      "17-files",
      () => addMessengerPickerAttachment(tray, seventeenFiles),
      "exactly one Messenger attachment",
    );

    const oversizedBytes = new Uint8Array(MESSENGER_ATTACHMENT_MAX_BYTES + 1);
    console.log(`TASK1186 over_8_MB_bytes=${oversizedBytes.byteLength} max_bytes=${MESSENGER_ATTACHMENT_MAX_BYTES}`);
    expectRefusedWithoutMutation(
      "over-8-MB",
      () => addMessengerDroppedAttachment(tray, [file("messenger-over-8-MB.bin", oversizedBytes)]),
      "8 MiB or smaller",
    );

    const folder = {
      ...file("messenger-folder-1186", new Uint8Array()),
      kind: "folder" as const,
    };
    console.log(`TASK1186 attempted_kind=${folder.kind}`);
    expectRefusedWithoutMutation(
      "folder",
      () => addMessengerPickerAttachment(tray, [folder]),
      "files, not folders",
    );

    console.log(`TASK1186 final_attached_count=${tray.cards.length}`);
    console.log(`TASK1186 final_attached_name=${tray.cards[0]?.name ?? "missing"}`);
    expect(tray.cards).toEqual(originalCards);
    expect(tray.cards).toHaveLength(1);
    expect(tray.cards[0]?.name).toBe("messenger-file-1186.txt");
  });
});
