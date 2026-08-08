import { describe, expect, it } from "vitest";
import {
  acceptDroppedFilesIntoTray,
  attachmentTrayMarkup,
  createOslChatAttachmentTray,
  setOslChatAttachmentUploadProgress,
} from "./chat-attachment-drop";

function displayedCounter(markup: string, name: "uploaded" | "total" | "completed"): number {
  const value = markup.match(new RegExp(`data-osl-chat-${name === "completed" ? "completed-pieces" : `${name}-bytes`}="(\\d+)"`, "u"))?.[1];
  if (!value) throw new Error(`missing visible ${name} counter`);
  return Number(value);
}

describe("TASK 0650 - connect upload progress display", () => {
  it("shows a throttled 37-byte upload on its tray card and clears the visible uploaded count", () => {
    const tray = createOslChatAttachmentTray();
    acceptDroppedFilesIntoTray(tray, [{ name: "throttled-37.bin", size: 37 }]);
    const trayId = tray.attachments[0]?.trayId;
    if (!trayId) throw new Error("37-byte fixture did not create a tray card");

    // This is the fixed screen snapshot taken after the throttled uploader has
    // read one 17-byte buffer, before it can finish the 37-byte fixture.
    setOslChatAttachmentUploadProgress(tray, trayId, {
      uploadedBytes: 17,
      totalBytes: 37,
      completedPieces: 17,
    });
    const liveMarkup = attachmentTrayMarkup(tray);
    const uploadedBytes = displayedCounter(liveMarkup, "uploaded");
    const totalBytes = displayedCounter(liveMarkup, "total");
    const completedPieces = displayedCounter(liveMarkup, "completed");

    console.log(`TASK0650 live uploaded_bytes=${uploadedBytes} total_bytes=${totalBytes} completed_pieces=${completedPieces}`);
    expect(uploadedBytes).toBeGreaterThan(0);
    expect(uploadedBytes).toBeLessThan(37);
    expect(totalBytes).toBe(37);
    expect(completedPieces).toBe(uploadedBytes);

    setOslChatAttachmentUploadProgress(tray, trayId, {
      uploadedBytes: 0,
      totalBytes: 0,
      completedPieces: 0,
    });
    const clearedUploadedBytes = displayedCounter(attachmentTrayMarkup(tray), "uploaded");
    console.log(`TASK0650 cleared uploaded_bytes=${clearedUploadedBytes}`);
    expect(clearedUploadedBytes).toBe(0);
  });
});
