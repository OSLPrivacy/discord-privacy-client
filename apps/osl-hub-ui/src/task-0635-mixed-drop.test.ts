import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { FOLDER_DROP_REJECTION, directPrivateTypingDropCommand, type DroppedAttachmentFile } from "./private-typing-drop-target";

const MIXED_DROP_FIXTURE: readonly DroppedAttachmentFile[] = [
  { name: "shoreline.png", type: "image/png", size: 2_048 },
  { name: "arrival.mp4", type: "video/mp4", size: 8_192 },
  { name: "itinerary.pdf", type: "application/pdf", size: 1_024 },
  { name: "trip-assets", type: "", size: 0, isDirectory: true },
];

describe("TASK 0635 mixed direct drop", () => {
  it("keeps three file records and rejects the folder from one direct command", () => {
    const tray = createAttachmentTrayActions();
    let directCommandCalls = 0;
    let folderRefusals = 0;
    let folderRefusal = "";

    try {
      directCommandCalls += 1;
      directPrivateTypingDropCommand(MIXED_DROP_FIXTURE, tray);
    } catch (error) {
      folderRefusal = error instanceof Error ? error.message : String(error);
      if (folderRefusal === FOLDER_DROP_REJECTION) folderRefusals += 1;
    }

    const cards = tray.getCards();
    console.log(`TASK0635_DIRECT_COMMAND_CALLS=${directCommandCalls}`);
    console.log(`TASK0635_TRAY_RECORD_COUNT=${cards.length}`);
    console.log(`TASK0635_TRAY_RECORDS=${cards.map((card) => `${card.name}:${card.type}`).join(",")}`);
    console.log(`TASK0635_FOLDER_REJECTION_COUNT=${folderRefusals}`);
    console.log(`TASK0635_FOLDER_REJECTION=${folderRefusal}`);

    expect(directCommandCalls).toBe(1);
    expect(cards).toHaveLength(3);
    expect(cards.map((card) => [card.name, card.type])).toEqual([
      ["shoreline.png", "image/png"],
      ["arrival.mp4", "video/mp4"],
      ["itinerary.pdf", "application/pdf"],
    ]);
    expect(folderRefusals).toBe(1);
    expect(folderRefusal).toBe(FOLDER_DROP_REJECTION);
  });
});
