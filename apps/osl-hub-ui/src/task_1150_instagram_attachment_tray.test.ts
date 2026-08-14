import { describe, expect, it } from "vitest";
import {
  INSTAGRAM_ATTACHMENT_MAX_BYTES,
  INSTAGRAM_ATTACHMENT_TRAY_MAX_FILES,
  addInstagramDroppedFiles,
  addInstagramPickerFiles,
  createInstagramAttachmentTray,
} from "./instagram-attachment-tray";

const bytes = new TextEncoder().encode("TASK 1150: picker and drop must agree exactly.");
const fixture = (name = "task-1150-instagram.png") => ({
  name,
  size: bytes.byteLength,
  arrayBuffer: async () => bytes.slice().buffer,
});

describe("TASK 1150 - Instagram attachment picker and drop tray", () => {
  it("adds one matching card from each intake path without sending", async () => {
    const pickerTray = createInstagramAttachmentTray();
    const dropTray = createInstagramAttachmentTray();

    const [pickerCard] = await addInstagramPickerFiles(pickerTray, [fixture()]);
    const [dropCard] = await addInstagramDroppedFiles(dropTray, [fixture()]);

    console.log(`TASK1150_PICKER_CARD_COUNT=${pickerTray.cards.length}`);
    console.log(`TASK1150_DROP_CARD_COUNT=${dropTray.cards.length}`);
    console.log(`TASK1150_PICKER_NAME=${pickerCard?.name}`);
    console.log(`TASK1150_DROP_NAME=${dropCard?.name}`);
    console.log(`TASK1150_PICKER_SIZE=${pickerCard?.sizeBytes}`);
    console.log(`TASK1150_DROP_SIZE=${dropCard?.sizeBytes}`);
    console.log(`TASK1150_PICKER_FINGERPRINT=${pickerCard?.fingerprint}`);
    console.log(`TASK1150_DROP_FINGERPRINT=${dropCard?.fingerprint}`);
    console.log(`TASK1150_PICKER_SEND_COUNT=${pickerTray.sendCount}`);
    console.log(`TASK1150_DROP_SEND_COUNT=${dropTray.sendCount}`);

    expect(pickerTray.cards).toHaveLength(1);
    expect(dropTray.cards).toHaveLength(1);
    expect(pickerCard).toMatchObject({ name: dropCard?.name, sizeBytes: dropCard?.sizeBytes, fingerprint: dropCard?.fingerprint });
    expect(pickerTray.sendCount).toBe(0);
    expect(dropTray.sendCount).toBe(0);
  });

  it("enforces the 16-file and 8 MiB tray gates", async () => {
    const tray = createInstagramAttachmentTray();
    await addInstagramPickerFiles(tray, Array.from({ length: INSTAGRAM_ATTACHMENT_TRAY_MAX_FILES }, (_, index) => fixture(`file-${index}`)));
    await expect(addInstagramDroppedFiles(tray, [fixture("seventeenth")])).rejects.toThrow("at most 16 files");
    const tooLarge = { ...fixture("too-large"), size: INSTAGRAM_ATTACHMENT_MAX_BYTES + 1 };
    await expect(addInstagramDroppedFiles(createInstagramAttachmentTray(), [tooLarge])).rejects.toThrow("8 MiB");
  });
});
