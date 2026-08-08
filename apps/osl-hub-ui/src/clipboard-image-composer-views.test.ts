import { describe, expect, it } from "vitest";
import { directClipboardImageComposerViews, registerClipboardImagePasting } from "./clipboard-image-composer-views";

type Listener = (event: Event) => void;

describe("TASK 0638 clipboard image composer views", () => {
  it("reports clipboard image support for every direct OSL app composer", async () => {
    const views = directClipboardImageComposerViews();
    const listeners = new Map<string, Listener>();
    const accepted: string[] = [];
    registerClipboardImagePasting(
      (selector) => ({ addEventListener: (type, listener) => { if (type === "paste") listeners.set(selector, listener as Listener); } }),
      async (view, bytes, mimeType) => { accepted.push(`${view.viewId}:${mimeType}:${bytes}`); },
    );

    for (const view of views) {
      const listener = listeners.get(view.composerSelector);
      expect(listener, `${view.viewId} has a paste listener`).toBeDefined();
      listener?.({
        preventDefault: () => undefined,
        clipboardData: { items: [{ kind: "file", type: "image/png", getAsFile: () => ({ type: "image/png", arrayBuffer: async () => new Uint8Array([137, 80, 78, 71]).buffer }) }] },
      } as unknown as Event);
    }
    await new Promise((resolve) => setTimeout(resolve, 0));

    console.log(`TASK_0638_DIRECT_VIEW_LIST=${views.map((view) => `${view.viewId}:${view.composerSelector}:clipboard-image`).join(",")}`);
    console.log(`TASK_0638_COMPOSER_COUNT=${views.length}`);
    console.log(`TASK_0638_REGISTERED_IMAGE_PASTES=${accepted.length}`);
    expect(views.map((view) => view.viewId)).toEqual(["osl-chat", "osl-mail"]);
    expect(accepted).toHaveLength(views.length);
  });
});
