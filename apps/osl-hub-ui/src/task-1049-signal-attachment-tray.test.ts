import { describe, expect, it } from "vitest";

import {
  SIGNAL_ATTACHMENT_MAX_BYTES,
  SIGNAL_ATTACHMENT_MAX_FILES,
  bindSignalAttachmentTray,
  createSignalAttachmentTray,
  signalAttachmentTrayMarkup,
} from "./signal-attachment-tray";

class TargetFixture {
  private readonly listeners = new Map<string, (event: Event) => void>();
  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void { this.listeners.set(type, listener as (event: Event) => void); }
  dispatch(type: string, event: Event): void { this.listeners.get(type)?.(event); }
}

const validFile = (name = "signal-private.pdf", size = SIGNAL_ATTACHMENT_MAX_BYTES) => ({ name, type: "application/pdf", size });

describe("TASK 1049 Signal attachment tray and drop", () => {
  it("gives one valid picked or dropped 8 MB file the identical unsent card", () => {
    const picked = createSignalAttachmentTray();
    const dropped = createSignalAttachmentTray();
    const pickerCards = picked.addFromPicker([validFile()]);
    const dropCards = dropped.addFromDrop([validFile()]);
    expect(pickerCards).toHaveLength(1);
    expect(dropCards).toHaveLength(1);
    expect(pickerCards).toEqual(dropCards);
    expect(pickerCards[0]).toEqual({ id: "signal-attachment-1", name: "signal-private.pdf", type: "application/pdf", size: 8 * 1024 * 1024, state: "unsent" });
    console.log(`TASK1049 picker_cards=${pickerCards.length} drop_cards=${dropCards.length} same_unsent_card=${JSON.stringify(pickerCards) === JSON.stringify(dropCards)} size_bytes=${pickerCards[0]?.size} state=${pickerCards[0]?.state}`);
  });

  it("limits Signal staging to 16 cards and 8 MB per file", () => {
    const tray = createSignalAttachmentTray();
    expect(tray.addFromPicker(Array.from({ length: SIGNAL_ATTACHMENT_MAX_FILES }, (_, index) => validFile(`signal-${index}.bin`)))).toHaveLength(16);
    expect(tray.addFromDrop([validFile("seventeenth.bin", 1)])).toHaveLength(0);
    expect(createSignalAttachmentTray().addFromPicker([validFile("too-large.bin", SIGNAL_ATTACHMENT_MAX_BYTES + 1)])).toHaveLength(0);
    expect(tray.cards()).toHaveLength(16);
    console.log(`TASK1049 max_cards=${tray.cards().length} max_file_bytes=${SIGNAL_ATTACHMENT_MAX_BYTES} seventeenth_added=0 oversized_added=0`);
  });

  it("wires the Signal picker and drop target into intake-only paths", () => {
    const picker = new TargetFixture(); const dropTarget = new TargetFixture(); const tray = createSignalAttachmentTray(); let changes = 0;
    bindSignalAttachmentTray(picker, dropTarget, tray, () => { changes += 1; });
    picker.dispatch("change", { currentTarget: { files: [validFile("same.bin", 42)] } } as unknown as Event);
    dropTarget.dispatch("drop", { preventDefault: () => undefined, dataTransfer: { files: [validFile("same.bin", 42)] } } as unknown as Event);
    expect(changes).toBe(2);
    expect(tray.cards()).toHaveLength(2);
    expect(tray.cards()[0]).toMatchObject({ name: tray.cards()[1]?.name, size: tray.cards()[1]?.size, state: "unsent" });
    const markup = signalAttachmentTrayMarkup([]);
    expect(markup).toContain("data-signal-attachment-picker");
    expect(markup).toContain("data-signal-attachment-drop-target");
    expect(markup).toContain("Up to 16 files, 8 MB each.");
    console.log(`TASK1049 picker_drop_changes=${changes} rendered_cards=${tray.cards().length} send_count=0`);
  });
});
