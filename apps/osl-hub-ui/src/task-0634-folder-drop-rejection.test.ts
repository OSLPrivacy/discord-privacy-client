import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { directPrivateTypingDropCommand, FOLDER_DROP_REJECTION } from "./private-typing-drop-target";

describe("TASK 0634 folder drop rejection", () => {
  it("exits 1 with the folder refusal before a tray item exists", () => {
    const tray = createAttachmentTrayActions();
    let exitCode = 0;
    let output = "";
    try {
      directPrivateTypingDropCommand([{ name: "private", type: "", size: 0, isDirectory: true }], tray);
    } catch (error) {
      exitCode = 1;
      output = error instanceof Error ? error.message : String(error);
    }
    console.log(`TASK0634 folder_drop_exit=${exitCode} message=${output} tray_count=${tray.getCards().length}`);
    expect(exitCode).toBe(1);
    expect(output).toBe(FOLDER_DROP_REJECTION);
    expect(tray.getCards()).toHaveLength(0);
  });
});
