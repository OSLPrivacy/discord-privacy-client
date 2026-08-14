import { describe, expect, it } from "vitest";

import {
  SIGNAL_ATTACHMENT_MAX_BYTES,
  SIGNAL_ATTACHMENT_MAX_FILES,
  createSignalAttachmentTray,
  type SignalAttachmentFile,
} from "./signal-attachment-tray";

const validFile = (name: string, size = 1): SignalAttachmentFile => ({
  name,
  type: "application/octet-stream",
  size,
});

describe("TASK 1050 - break Signal attachment limits", () => {
  it("refuses 17 files, an over-8-MB file, and a folder while preserving valid unsent cards", () => {
    const tray = createSignalAttachmentTray();
    const [goodCard] = tray.addFromPicker([validFile("valid-signal-card.bin")]);
    expect(goodCard).toMatchObject({ name: "valid-signal-card.bin", state: "unsent" });
    console.log(`TASK1050_INITIAL_VALID_CARDS=${tray.cards().length} STATE=${goodCard?.state}`);

    const seventeenFiles = Array.from(
      { length: SIGNAL_ATTACHMENT_MAX_FILES + 1 },
      (_, index) => validFile(`file-${index + 1}.bin`),
    );
    const addedSeventeen = tray.addFromPicker(seventeenFiles);
    expect(addedSeventeen).toHaveLength(0);
    console.log(`TASK1050_17_FILES_ATTEMPTED=${seventeenFiles.length} REFUSED=${addedSeventeen.length === 0}`);

    const overEightMbBytes = SIGNAL_ATTACHMENT_MAX_BYTES + 1;
    const addedOversized = tray.addFromDrop([validFile("over-8-MB.bin", overEightMbBytes)]);
    expect(addedOversized).toHaveLength(0);
    console.log(`TASK1050_OVER_8_MB_FILE_BYTES=${overEightMbBytes} REFUSED=${addedOversized.length === 0}`);

    const folder: SignalAttachmentFile = { ...validFile("folder"), isDirectory: true };
    const addedFolder = tray.addFromDrop([folder]);
    expect(addedFolder).toHaveLength(0);
    console.log(`TASK1050_FOLDER_ATTEMPTED=${folder.name} REFUSED=${addedFolder.length === 0}`);

    expect(tray.cards()).toEqual([goodCard]);
    expect(tray.cards()[0]?.state).toBe("unsent");
    console.log(`TASK1050_FINAL_VALID_CARDS=${tray.cards().length} STATE=${tray.cards()[0]?.state} VALID_CARDS_UNSENT=true`);
  });
});
