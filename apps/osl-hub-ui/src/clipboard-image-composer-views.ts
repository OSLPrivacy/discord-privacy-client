/**
 * The direct, first-party OSL composers that accept an image pasted from the
 * desktop clipboard.  Keep this list beside the registration code so a newly
 * added composer cannot silently miss the intake path.
 */
export const OSL_CLIPBOARD_IMAGE_COMPOSER_VIEWS = [
  { viewId: "osl-chat", composerSelector: "#osl-chat-draft" },
  { viewId: "osl-mail", composerSelector: "#osl-mail-body" },
] as const;

export type OslClipboardImageComposerView = typeof OSL_CLIPBOARD_IMAGE_COMPOSER_VIEWS[number];

/** A deliberately direct list for capability checks and release evidence. */
export function directClipboardImageComposerViews(): readonly OslClipboardImageComposerView[] {
  return OSL_CLIPBOARD_IMAGE_COMPOSER_VIEWS;
}

type PasteTarget = Pick<EventTarget, "addEventListener">;
type ClipboardImageFile = { type: string; arrayBuffer(): Promise<ArrayBuffer> };
type ClipboardPasteEvent = Event & { clipboardData: { items: Array<{ kind: string; type: string; getAsFile(): ClipboardImageFile | null }> } | null };

function pastedImage(event: ClipboardPasteEvent): ClipboardImageFile | null {
  const item = [...(event.clipboardData?.items ?? [])]
    .find((candidate) => candidate.kind === "file" && (candidate.type === "image/png" || candidate.type === "image/jpeg"));
  return item?.getAsFile() ?? null;
}

function base64(bytes: ArrayBuffer): string {
  let text = "";
  for (const byte of new Uint8Array(bytes)) text += String.fromCharCode(byte);
  return btoa(text);
}

/** Register the same image-only paste intake on every first-party composer. */
export function registerClipboardImagePasting(
  find: (selector: string) => PasteTarget | null,
  intake: (view: OslClipboardImageComposerView, imageBytesB64: string, mimeType: "image/png" | "image/jpeg") => Promise<void>,
): void {
  for (const view of directClipboardImageComposerViews()) {
    find(view.composerSelector)?.addEventListener("paste", (rawEvent) => {
      const event = rawEvent as ClipboardPasteEvent;
      const image = pastedImage(event);
      if (!image) return;
      event.preventDefault();
      void image.arrayBuffer().then((bytes) => intake(view, base64(bytes), image.type as "image/png" | "image/jpeg"));
    });
  }
}
