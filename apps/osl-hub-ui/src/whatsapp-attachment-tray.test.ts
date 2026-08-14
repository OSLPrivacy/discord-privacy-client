import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  addDroppedWhatsAppAttachmentFiles,
  addPickedWhatsAppAttachmentFiles,
  WHATSAPP_ATTACHMENT_MAX_BYTES,
  WHATSAPP_ATTACHMENT_MAX_FILES,
} from "./whatsapp-attachment-tray";

describe("WhatsApp attachment tray", () => {
  it("gives a picked file and a dropped file the same unsent tray card", () => {
    const file = { name: "private-photo.png", size: 640 * 1024 };
    const picked = addPickedWhatsAppAttachmentFiles(undefined, [file]);
    const dropped = addDroppedWhatsAppAttachmentFiles(undefined, [file]);

    expect(picked.cards).toEqual(dropped.cards);
    expect(picked.cards).toEqual([{ name: "private-photo.png", size: 640 * 1024, sizeLabel: "0.63 MiB", state: "unsent" }]);
    expect(picked.rejected).toEqual([]);
    expect(dropped.rejected).toEqual([]);
  });

  it("keeps a 16-file, 8 MiB-per-file unsent tray boundary", () => {
    const file = (index: number, size = WHATSAPP_ATTACHMENT_MAX_BYTES) => ({ name: `file-${index}.bin`, size });
    const accepted = addPickedWhatsAppAttachmentFiles(undefined, Array.from({ length: WHATSAPP_ATTACHMENT_MAX_FILES }, (_, index) => file(index)));
    const tooMany = addDroppedWhatsAppAttachmentFiles(accepted, [file(16)]);
    const tooLarge = addDroppedWhatsAppAttachmentFiles(undefined, [file(0, WHATSAPP_ATTACHMENT_MAX_BYTES + 1)]);

    expect(accepted.cards).toHaveLength(16);
    expect(tooMany.cards).toHaveLength(16);
    expect(tooMany.rejected).toEqual(["The attachment tray holds up to 16 files."]);
    expect(tooLarge.cards).toEqual([]);
    expect(tooLarge.rejected).toEqual(["file-0.bin is larger than 8 MiB."]);
  });

  it("wires the overlay picker and drop event into the same tray admission paths", () => {
    const overlay = readFileSync(new URL("./whatsapp-overlay.ts", import.meta.url), "utf8");
    const html = readFileSync(new URL("../whatsapp-overlay.html", import.meta.url), "utf8");

    expect(html).toContain('id="whatsapp-attachment-picker"');
    expect(html).toContain('id="whatsapp-attachment-input"');
    expect(html).toContain('id="whatsapp-attachment-tray"');
    expect(overlay).toContain('stageAttachments(attachmentInput.files ?? [], "picker")');
    expect(overlay).toContain('stageAttachments(event.dataTransfer?.files ?? [], "drop")');
    expect(overlay).toContain('· Unsent');
  });
});
