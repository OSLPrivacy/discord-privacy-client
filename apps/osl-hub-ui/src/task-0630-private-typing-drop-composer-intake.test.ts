import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import {
  directPrivateTypingDropComposerViews,
  registerPrivateTypingDropIntake,
  type OslPrivateTypingDropComposerView,
} from "./private-typing-drop-composer-views";

type Listener = (event: Event) => void;

class ComposerCommandFixture {
  private readonly listeners = new Map<string, Listener>();
  private readonly tray = createAttachmentTrayActions();
  private sends = 0;

  readonly target = {
    addEventListener: (type: string, listener: EventListenerOrEventListenerObject): void => {
      this.listeners.set(type, listener as Listener);
    },
  };

  dropImage(name: string): void {
    this.listeners.get("drop")?.({
      preventDefault: () => undefined,
      dataTransfer: { files: [{ name, type: "image/png", size: 17 }] },
    } as unknown as Event);
  }

  trayCount(): number {
    return this.tray.getCards().length;
  }

  sendCount(): number {
    return this.sends;
  }

  attachmentTray() {
    return this.tray;
  }
}

describe("TASK 0630 private typing drop composer intake", () => {
  it("drops one image through each registered composer command without sending", () => {
    const views = directPrivateTypingDropComposerViews();
    expect(views.map((view) => view.viewId)).toEqual(["osl-chat", "osl-mail"]);
    const composers = new Map<string, ComposerCommandFixture>(
      views.map((view) => [view.viewId, new ComposerCommandFixture()]),
    );

    registerPrivateTypingDropIntake(
      (selector) => {
        const view = views.find((candidate) => candidate.composerSelector === selector);
        return view ? composers.get(view.viewId)!.target : null;
      },
      (view: OslPrivateTypingDropComposerView) => composers.get(view.viewId)!.attachmentTray(),
      () => undefined,
    );

    for (const view of views) {
      const composer = composers.get(view.viewId)!;
      composer.dropImage(`${view.viewId}.png`);

      const trayCount = composer.trayCount();
      const sendCount = composer.sendCount();
      console.log(`TASK0630_COMPOSER=${view.viewId} tray_count=${trayCount} send_count=${sendCount}`);
      expect(trayCount, `${view.viewId} tray_count`).toBe(1);
      expect(sendCount, `${view.viewId} send_count`).toBe(0);
    }
  });
});
