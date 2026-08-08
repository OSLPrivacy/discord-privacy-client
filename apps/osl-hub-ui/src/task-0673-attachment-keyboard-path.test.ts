import { describe, expect, it } from "vitest";

import { attachmentPickerLimit } from "./attachment-picker-limit";
import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { attachmentTrayScreenMarkup, type AttachmentTrayCard } from "./attachment-tray-screen";
import { registerClipboardImagePasting } from "./clipboard-image-composer-views";

const FIXTURE_BYTES = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 6, 7, 3]);
const FIXTURE_NAME = "keyboard-fixture.png";
const FIXTURE_TYPE = "image/png";

type NamedTrayFields = { name: string; type: string; size: number; count: number; limitMessage: string };

function trayFields(card: AttachmentTrayCard): NamedTrayFields {
  const tray = createAttachmentTrayActions([card]);
  const limitMessage = attachmentPickerLimit("free").hint;
  const markup = attachmentTrayScreenMarkup(tray.getCards());
  expect(markup).toContain(card.name);
  expect(markup).toContain(card.type);
  return { name: card.name, type: card.type, size: card.size, count: tray.getCards().length, limitMessage };
}

describe("TASK 0673 attachment keyboard and clipboard paths", () => {
  it("puts one keyboard-selected fixture and one pasted image into equivalent named tray fields and warnings", async () => {
    // The keyboard fixture represents a file selected after the file input has
    // keyboard focus. It deliberately uses the same bytes and MIME type as the
    // clipboard File so a difference can only come from the ingress path.
    const keyboard = trayFields({
      removableId: "keyboard-fixture",
      name: FIXTURE_NAME,
      type: FIXTURE_TYPE,
      size: FIXTURE_BYTES.byteLength,
      previewDataUrl: null,
    });

    let pasteListener: ((event: Event) => void) | undefined;
    let clipboard: NamedTrayFields | undefined;
    registerClipboardImagePasting(
      (selector) => selector === "#osl-chat-draft" ? { addEventListener: (_type, listener) => { pasteListener = listener as (event: Event) => void; } } : null,
      async (_view, bytesB64, mimeType) => {
        const bytes = Uint8Array.from(atob(bytesB64), (character) => character.charCodeAt(0));
        clipboard = trayFields({
          removableId: "clipboard-fixture",
          name: FIXTURE_NAME,
          type: mimeType,
          size: bytes.byteLength,
          previewDataUrl: null,
        });
      },
    );
    expect(pasteListener).toBeDefined();
    pasteListener?.({
      preventDefault: () => undefined,
      clipboardData: { items: [{ kind: "file", type: FIXTURE_TYPE, getAsFile: () => ({ type: FIXTURE_TYPE, arrayBuffer: async () => FIXTURE_BYTES.buffer }) }] },
    } as unknown as Event);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(clipboard).toBeDefined();
    expect(keyboard).toEqual(clipboard);
    for (const [field, value] of Object.entries(keyboard)) expect(value, `${field} is non-empty`).not.toBeFalsy();
    console.log(`TASK0673_KEYBOARD fields=${JSON.stringify(keyboard)}`);
    console.log(`TASK0673_CLIPBOARD fields=${JSON.stringify(clipboard)}`);
  });
});
