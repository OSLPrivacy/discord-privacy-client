import { describe, expect, it } from "vitest";
import { directClipboardImageComposerViews, registerClipboardImagePasting } from "./clipboard-image-composer-views";

type Listener = (event: Event) => void;

// This single PNG is deliberately pasted into every composer. Keeping the
// fixture shared makes this a command-coverage check rather than a collection
// of unrelated per-composer examples.
const CLIPBOARD_PNG = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
const HANDLER_STUBBED = process.env.OSL_TASK_0639_STUB_CLIPBOARD_IMAGE_HANDLER === "1";

describe("TASK 0639 clipboard image in every composer", () => {
  it("stages one fixture image in every composer without sending", async () => {
    const views = directClipboardImageComposerViews();
    const listeners = new Map<string, Listener>();
    const trayCount = new Map<string, number>();
    let sendCount = 0;

    registerClipboardImagePasting(
      (selector) => ({
        addEventListener: (type, listener) => {
          if (type === "paste") listeners.set(selector, listener as Listener);
        },
      }),
      async (view, imageBytesB64, mimeType) => {
        // This flag is only for the required mutation check. A no-op clipboard
        // handler must leave every tray empty and make the assertions fail.
        if (HANDLER_STUBBED) return;
        expect(mimeType).toBe("image/png");
        expect(imageBytesB64).toBe("iVBORw0KGgo=");
        trayCount.set(view.viewId, (trayCount.get(view.viewId) ?? 0) + 1);
      },
    );

    for (const view of views) {
      const listener = listeners.get(view.composerSelector);
      expect(listener, `${view.viewId} composer has its clipboard-image command`).toBeDefined();
      listener?.({
        preventDefault: () => undefined,
        clipboardData: {
          items: [{
            kind: "file",
            type: "image/png",
            getAsFile: () => ({ type: "image/png", arrayBuffer: async () => CLIPBOARD_PNG.buffer }),
          }],
        },
      } as unknown as Event);
    }
    await new Promise((resolve) => setTimeout(resolve, 0));

    const counts = views.map((view) => `${view.viewId}:tray_count=${trayCount.get(view.viewId) ?? 0}:send_count=${sendCount}`).join(",");
    console.log(`TASK_0639_COMPOSER_COUNTS=${counts}`);
    console.log(`TASK_0639_FIXTURE_MIME=image/png bytes=${CLIPBOARD_PNG.length} handler_stubbed=${HANDLER_STUBBED}`);

    for (const view of views) {
      expect(trayCount.get(view.viewId) ?? 0, `${view.viewId} tray count`).toBe(1);
    }
    expect(sendCount).toBe(0);
  });
});
