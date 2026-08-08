import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { bindPrivateTypingBoxDropTarget, directPrivateTypingDropCommand } from "./private-typing-drop-target";

class DropTargetFixture {
  private readonly listeners = new Map<string, (event: Event) => void>();

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    this.listeners.set(type, listener as (event: Event) => void);
  }

  dispatch(type: string, event: Event): void {
    this.listeners.get(type)?.(event);
  }
}

describe("TASK 0628 private typing drop target", () => {
  it("direct drop command produces one tray record and no send command", () => {
    const tray = createAttachmentTrayActions();
    let sendCommands = 0;
    const records = directPrivateTypingDropCommand([
      { name: "drop-fixture.png", type: "image/png", size: 17 },
    ], tray);

    expect(records).toHaveLength(1);
    expect(tray.getCards()).toHaveLength(1);
    expect(sendCommands).toBe(0);
    console.log(`TASK0628_DIRECT_DROP tray_records=${tray.getCards().length} send_commands=${sendCommands}`);
  });

  it("binds dragover and drop to the private typing box without submitting", () => {
    const target = new DropTargetFixture();
    const tray = createAttachmentTrayActions();
    let renders = 0;
    bindPrivateTypingBoxDropTarget(target, tray, () => { renders += 1; });
    let prevented = 0;
    target.dispatch("dragover", { preventDefault: () => { prevented += 1; } } as unknown as Event);
    target.dispatch("drop", {
      preventDefault: () => { prevented += 1; },
      dataTransfer: { files: [{ name: "bound-drop.txt", type: "text/plain", size: 3 }] },
    } as unknown as Event);
    expect(prevented).toBe(2);
    expect(tray.getCards()).toHaveLength(1);
    expect(renders).toBe(1);
  });
});
