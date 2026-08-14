import { describe, expect, it } from "vitest";
import {
  INSTAGRAM_ATTACHMENT_MAX_BYTES,
  INSTAGRAM_ATTACHMENT_TRAY_MAX_FILES,
  InstagramAttachmentRefusal,
  addInstagramPickerFiles,
  createInstagramAttachmentTray,
  type InstagramAttachmentFile,
  type InstagramAttachmentRefusalName,
} from "./instagram-attachment-tray";

const goodBytes = new TextEncoder().encode("instagram-file-1151.jpg");

const file = (
  name: string,
  size = goodBytes.byteLength,
  isDirectory = false,
): InstagramAttachmentFile => ({
  name,
  size,
  isDirectory,
  arrayBuffer: async () => goodBytes.slice().buffer,
});

async function expectRefusedByName(
  attempt: Promise<unknown>,
  expectedName: InstagramAttachmentRefusalName,
): Promise<void> {
  try {
    await attempt;
    throw new Error(`expected ${expectedName} to be refused`);
  } catch (error: unknown) {
    expect(error).toBeInstanceOf(InstagramAttachmentRefusal);
    expect(error).toMatchObject({ refusalName: expectedName });
    expect((error as Error).message).toContain(expectedName);
    console.log(`TASK1151_CHANGED_ATTACHMENT_VALUE=${expectedName} REFUSED_BY_NAME=${expectedName}`);
  }
}

describe("TASK 1151 - break Instagram attachment limits", () => {
  it("refuses 17 files, an over-8-MB file, and a folder without changing the good attachment", async () => {
    const tray = createInstagramAttachmentTray();
    const goodName = "instagram-file-1151.jpg";

    const [goodCard] = await addInstagramPickerFiles(tray, [file(goodName)]);
    expect(tray.cards).toHaveLength(1);
    expect(goodCard?.name).toBe(goodName);
    console.log(`TASK1151_INITIAL_ATTACHED_FILE_COUNT=${tray.cards.length}`);
    console.log(`TASK1151_INITIAL_ATTACHED_FILE_NAME=${goodCard?.name}`);

    const seventeenFiles = Array.from(
      { length: INSTAGRAM_ATTACHMENT_TRAY_MAX_FILES + 1 },
      (_, index) => file(`changed-${index + 1}.jpg`),
    );
    console.log(`TASK1151_17_FILES_ATTEMPTED=${seventeenFiles.length}`);
    await expectRefusedByName(addInstagramPickerFiles(tray, seventeenFiles), "17-files");

    const overEightMbBytes = INSTAGRAM_ATTACHMENT_MAX_BYTES + 1;
    console.log(`TASK1151_OVER_8_MB_FILE_BYTES=${overEightMbBytes}`);
    await expectRefusedByName(
      addInstagramPickerFiles(tray, [file("changed-too-large.jpg", overEightMbBytes)]),
      "over-8-MB",
    );

    console.log("TASK1151_FOLDER_ATTEMPTED=changed-folder");
    await expectRefusedByName(
      addInstagramPickerFiles(tray, [file("changed-folder", goodBytes.byteLength, true)]),
      "folder",
    );

    expect(tray.cards).toHaveLength(1);
    expect(tray.cards[0]).toEqual(goodCard);
    expect(tray.cards[0]?.name).toBe(goodName);
    console.log(`TASK1151_FINAL_ATTACHED_FILE_COUNT=${tray.cards.length}`);
    console.log(`TASK1151_FINAL_ATTACHED_FILE_NAME=${tray.cards[0]?.name}`);
    console.log("TASK1151_GOOD_ATTACHMENT_UNCHANGED=true");
  });
});
