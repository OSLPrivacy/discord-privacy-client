import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { directPrivateTypingDropComposerViews, registerPrivateTypingDropIntake } from "./private-typing-drop-composer-views";

type Listener = (event: Event) => void;

describe("TASK 0629 private typing drop composer views", () => {
  it("reports drag-drop enabled for every direct OSL app composer", () => {
    const views = directPrivateTypingDropComposerViews();
    const listeners = new Map<string, Listener>();
    const trays = new Map(views.map((view) => [view.viewId, createAttachmentTrayActions()]));

    registerPrivateTypingDropIntake(
      (selector) => ({ addEventListener: (type, listener) => { if (type === "drop") listeners.set(selector, listener as Listener); } }),
      (view) => trays.get(view.viewId)!,
      () => undefined,
    );

    for (const view of views) {
      const listener = listeners.get(view.composerSelector);
      expect(listener, `${view.viewId} has a drop listener`).toBeDefined();
      listener?.({
        preventDefault: () => undefined,
        dataTransfer: { files: [{ name: `${view.viewId}.txt`, type: "text/plain", size: 1 }] },
      } as unknown as Event);
    }

    const registered = views.filter((view) => trays.get(view.viewId)?.getCards().length === 1).length;
    console.log(`TASK0629_DIRECT_VIEW_LIST=${views.map((view) => `${view.viewId}:${view.composerSelector}:drag-drop-enabled`).join(",")}`);
    console.log(`TASK0629_COMPOSER_COUNT=${views.length}`);
    console.log(`TASK0629_DRAG_DROP_ENABLED_COUNT=${registered}`);
    expect(views.map((view) => view.viewId)).toEqual(["osl-chat", "osl-mail"]);
    expect(registered).toBe(views.length);
  });
});
