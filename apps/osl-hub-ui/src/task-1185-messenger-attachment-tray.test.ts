import { describe, expect, it } from "vitest";
import {
  MESSENGER_ATTACHMENT_MAX_BYTES,
  MESSENGER_ATTACHMENT_TRAY_MAX_FILES,
  addMessengerDroppedAttachment,
  addMessengerPickerAttachment,
  createMessengerAttachmentTray,
  messengerAttachmentTrayMarkup,
  type MessengerAttachmentSource,
} from "./messenger-attachment-tray";

const bytes = new TextEncoder().encode("Messenger attachment fixture: blue finch\n");
const fixture: MessengerAttachmentSource = { name: "blue-finch.txt", size: bytes.byteLength, bytes };

describe("TASK 1185 - Messenger attachment picker and drop", () => {
  it("stages one matching card from picker and drop without sending", () => {
    const pickerTray = createMessengerAttachmentTray();
    const dropTray = createMessengerAttachmentTray();
    const picked = addMessengerPickerAttachment(pickerTray, [fixture]);
    const dropped = addMessengerDroppedAttachment(dropTray, [fixture]);

    console.log(`TASK1185 picker_added_cards=${picked.addedCardCount} drop_added_cards=${dropped.addedCardCount}`);
    console.log(`TASK1185 picker_name=${picked.card.name} drop_name=${dropped.card.name}`);
    console.log(`TASK1185 picker_size=${picked.card.size} drop_size=${dropped.card.size}`);
    console.log(`TASK1185 picker_fingerprint=${picked.card.fingerprint} drop_fingerprint=${dropped.card.fingerprint}`);
    console.log(`TASK1185 picker_send_count=${pickerTray.sendCount} drop_send_count=${dropTray.sendCount}`);

    expect(picked.addedCardCount).toBe(1);
    expect(dropped.addedCardCount).toBe(1);
    expect(pickerTray.cards).toHaveLength(1);
    expect(dropTray.cards).toHaveLength(1);
    expect(dropped.card).toMatchObject({ name: picked.card.name, size: picked.card.size, fingerprint: picked.card.fingerprint });
    expect(pickerTray.sendCount).toBe(0);
    expect(dropTray.sendCount).toBe(0);
    expect(messengerAttachmentTrayMarkup(pickerTray)).toContain('data-messenger-send-count="0"');
  });

  it("keeps the advertised 16-file and 8 MiB admission gates", () => {
    const tray = createMessengerAttachmentTray();
    for (let index = 0; index < MESSENGER_ATTACHMENT_TRAY_MAX_FILES; index += 1) {
      addMessengerPickerAttachment(tray, [{ ...fixture, name: `fixture-${index}.txt` }]);
    }
    expect(tray.cards).toHaveLength(16);
    expect(() => addMessengerPickerAttachment(tray, [fixture])).toThrow("at most 16 files");
    const sizeTray = createMessengerAttachmentTray();
    expect(() => addMessengerDroppedAttachment(sizeTray, [{ name: "large.bin", size: MESSENGER_ATTACHMENT_MAX_BYTES + 1, bytes: new Uint8Array(MESSENGER_ATTACHMENT_MAX_BYTES + 1) }])).toThrow("8 MiB or smaller");
  });
});
