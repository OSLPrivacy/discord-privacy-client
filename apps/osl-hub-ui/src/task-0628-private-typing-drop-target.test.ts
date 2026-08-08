import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { bindPrivateTypingBoxDropTarget, directPrivateTypingDropCommand, privateTypingDropOutlineState } from "./private-typing-drop-target";

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
    target.dispatch("dragover", {
      preventDefault: () => { prevented += 1; },
      dataTransfer: { files: [{ name: "bound-drop.txt", type: "text/plain", size: 3 }] },
    } as unknown as Event);
    target.dispatch("drop", {
      preventDefault: () => { prevented += 1; },
      dataTransfer: { files: [{ name: "bound-drop.txt", type: "text/plain", size: 3 }] },
    } as unknown as Event);
    expect(prevented).toBe(2);
    expect(tray.getCards()).toHaveLength(1);
    expect(renders).toBe(1);
  });

  it("reports the Drop to attach outline only while a file is over the typing box", () => {
    const target = new DropTargetFixture();
    const tray = createAttachmentTrayActions();
    const states = [privateTypingDropOutlineState(false)];
    bindPrivateTypingBoxDropTarget(target, tray, undefined, (state) => { states.push(state); });
    const event = {
      preventDefault: () => undefined,
      dataTransfer: { files: [{ name: "drag-over.txt", type: "text/plain", size: 1 }] },
    } as unknown as Event;
    target.dispatch("dragenter", event);
    target.dispatch("dragleave", event);

    expect(states).toEqual([
      { outlineVisible: false, text: null },
      { outlineVisible: true, text: "Drop to attach" },
      { outlineVisible: false, text: null },
    ]);
    console.log(`TASK0632_DROP_OUTLINE before=${states[0].outlineVisible} drag_over=${states[1].outlineVisible} after=${states[2].outlineVisible} text=${JSON.stringify(states[1].text)}`);
  });

  it("renders the drag-over state as a composer outline with its Drop to attach text", () => {
    const view = readFileSync(new URL("./osl-chats-view.ts", import.meta.url), "utf8");
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(view).toContain('class="osl-chat-drop-outline" aria-hidden="true">Drop to attach</span>');
    expect(styles).toContain(".osl-chat-composer-bar.is-file-drag-over");
    expect(styles).toContain(".osl-chat-composer-bar.is-file-drag-over .osl-chat-drop-outline");
  });
});
