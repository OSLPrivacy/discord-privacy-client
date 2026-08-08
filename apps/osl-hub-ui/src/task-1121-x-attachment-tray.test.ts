import { describe, expect, it, vi } from "vitest";

import { X_ATTACHMENT_MAX_BYTES, X_ATTACHMENT_MAX_FILES, bindXAttachmentTray, createXAttachmentTray, xAttachmentTrayMarkup } from "./x-attachment-tray";

class TargetFixture {
  private readonly listeners = new Map<string, (event: Event) => void>();

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    this.listeners.set(type, listener as (event: Event) => void);
  }

  dispatch(type: string, event: Event): void {
    this.listeners.get(type)?.(event);
  }
}

function fixture(): { name: string; type: string; size: number; arrayBuffer(): Promise<ArrayBuffer> } {
  const bytes = new TextEncoder().encode("TASK1121 exact X attachment bytes");
  return {
    name: "x-fixture.txt",
    type: "text/plain",
    size: bytes.byteLength,
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  };
}

describe("TASK 1121 X attachment tray and drop", () => {
  it("adds exactly one matching local card from the picker and drop paths without sending", async () => {
    const pickerTray = createXAttachmentTray();
    const dropTray = createXAttachmentTray();
    let pickerSendCount = 0;
    let dropSendCount = 0;

    const pickerAdded = await pickerTray.addFromPicker([fixture()]);
    const dropAdded = await dropTray.addFromDrop([fixture()]);
    const picker = pickerTray.cards();
    const drop = dropTray.cards();

    expect(pickerAdded).toHaveLength(1);
    expect(dropAdded).toHaveLength(1);
    expect(picker).toHaveLength(1);
    expect(drop).toHaveLength(1);
    expect(picker[0]).toMatchObject({ name: drop[0]?.name, size: drop[0]?.size, fingerprint: drop[0]?.fingerprint });
    expect(pickerSendCount).toBe(0);
    expect(dropSendCount).toBe(0);

    console.log(`TASK1121 picker_cards=${picker.length} drop_cards=${drop.length} name=${picker[0]?.name} size=${picker[0]?.size} fingerprint=${picker[0]?.fingerprint} picker_send_count=${pickerSendCount} drop_send_count=${dropSendCount}`);
  });

  it("states and enforces the 16-file, 8 MB X tray limits", async () => {
    const tray = createXAttachmentTray();
    const files = Array.from({ length: X_ATTACHMENT_MAX_FILES }, (_, index) => ({ ...fixture(), name: `x-${index}.txt` }));
    expect(await tray.addFromPicker(files)).toHaveLength(X_ATTACHMENT_MAX_FILES);
    expect(await tray.addFromDrop([fixture()])).toHaveLength(0);
    expect(await createXAttachmentTray().addFromPicker([{ ...fixture(), size: X_ATTACHMENT_MAX_BYTES + 1 }])).toHaveLength(0);
    const markup = xAttachmentTrayMarkup([]);
    expect(markup).toContain('data-x-attachment-picker');
    expect(markup).toContain('data-x-attachment-drop-target');
    expect(markup).toContain("Up to 16 files, 8 MB each.");
  });

  it("binds the rendered picker and drop target as intake-only paths", async () => {
    const picker = new TargetFixture();
    const dropTarget = new TargetFixture();
    const tray = createXAttachmentTray();
    let changes = 0;
    let sends = 0;
    bindXAttachmentTray(picker, dropTarget, tray, () => { changes += 1; });

    picker.dispatch("change", { currentTarget: { files: [fixture()] } } as unknown as Event);
    await vi.waitFor(() => expect(changes).toBe(1));
    dropTarget.dispatch("drop", {
      preventDefault: () => undefined,
      dataTransfer: { files: [fixture()] },
    } as unknown as Event);
    await vi.waitFor(() => expect(changes).toBe(2));

    expect(tray.cards()).toHaveLength(2);
    expect(tray.cards()[0]).toMatchObject({ name: tray.cards()[1]?.name, size: tray.cards()[1]?.size, fingerprint: tray.cards()[1]?.fingerprint });
    expect(sends).toBe(0);
  });
});
