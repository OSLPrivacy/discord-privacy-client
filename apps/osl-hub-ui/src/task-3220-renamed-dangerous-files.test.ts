import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { attachmentTrayScreenMarkup } from "./attachment-tray-screen";
import { directPrivateTypingDropCommand } from "./private-typing-drop-target";

const renamedDangerousFiles = [
  { name: "quarterly-plan.pdf", type: "application/pdf", size: 35_664 },
  { name: "meeting-agenda.md", type: "text/markdown", size: 192 },
  { name: "travel-budget.csv", type: "text/csv", size: 36_864 },
  { name: "shopping-list.txt", type: "text/plain", size: 111 },
  { name: "bookmarks.json", type: "application/json", size: 49 },
] as const;

describe("TASK 3220 attack 38 renamed dangerous files", () => {
  it("draws five metadata-only tray cards and no content preview", () => {
    const tray = createAttachmentTrayActions();
    const records = directPrivateTypingDropCommand(renamedDangerousFiles, tray);
    const markup = attachmentTrayScreenMarkup(tray.getCards());
    const previewDrawCount = markup.match(/<(?:audio|canvas|embed|iframe|img|object|picture|video)\b/giu)?.length ?? 0;
    const previewDataUrlCount = tray.getCards().filter((card) => card.previewDataUrl !== null).length;

    expect(records).toHaveLength(5);
    expect(tray.getCards()).toHaveLength(5);
    expect(tray.getCards().map((card) => card.name)).toEqual(renamedDangerousFiles.map((file) => file.name));
    expect(markup.match(/data-attachment-tray-card=/gu)).toHaveLength(5);
    expect(previewDrawCount).toBe(0);
    expect(previewDataUrlCount).toBe(0);

    console.log(`TASK3220_UI_TRAY_FILE_COUNT=${tray.getCards().length}`);
    console.log(`TASK3220_PREVIEW_DRAW_COUNT=${previewDrawCount}`);
    console.log(`TASK3220_PREVIEW_DATA_URL_COUNT=${previewDataUrlCount}`);
  });
});
