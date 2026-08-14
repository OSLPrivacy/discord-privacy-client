/**
 * TASK 1121 — X attachment intake.
 *
 * This is deliberately an intake-only model.  Choosing or dropping a file
 * creates a local tray card; the X send control remains a separate, explicit
 * operation.  Keeping the same admission path for both inputs means the
 * visible metadata and byte fingerprint cannot diverge by input method.
 */

export const X_ATTACHMENT_MAX_FILES = 16;
export const X_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;

export interface XAttachmentFile {
  readonly name: string;
  readonly type: string;
  readonly size: number;
  arrayBuffer(): Promise<ArrayBuffer>;
}

export interface XAttachmentTrayCard {
  readonly id: string;
  readonly name: string;
  readonly type: string;
  readonly size: number;
  /** SHA-256 of the exact selected bytes, lower-case hexadecimal. */
  readonly fingerprint: string;
}

export interface XAttachmentTray {
  cards(): readonly XAttachmentTrayCard[];
  addFromPicker(files: Iterable<XAttachmentFile>): Promise<readonly XAttachmentTrayCard[]>;
  addFromDrop(files: Iterable<XAttachmentFile>): Promise<readonly XAttachmentTrayCard[]>;
}

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

function validFile(file: XAttachmentFile): boolean {
  return file.name.trim().length > 0
    && Number.isSafeInteger(file.size)
    && file.size >= 0
    && file.size <= X_ATTACHMENT_MAX_BYTES;
}

/** Returns the stable byte identity displayed for an X tray item. */
export async function xAttachmentFingerprint(file: Pick<XAttachmentFile, "arrayBuffer">): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", await file.arrayBuffer());
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function cardsFromFiles(files: Iterable<XAttachmentFile>, startingAt: number): Promise<XAttachmentTrayCard[]> {
  const accepted = [...files].filter(validFile);
  if (accepted.length + startingAt > X_ATTACHMENT_MAX_FILES) return [];
  return Promise.all(accepted.map(async (file, index) => ({
    id: `x-attachment-${startingAt + index + 1}`,
    name: file.name,
    type: file.type || "application/octet-stream",
    size: file.size,
    fingerprint: await xAttachmentFingerprint(file),
  })));
}

/** Creates an X-specific local tray with one shared picker/drop admission path. */
export function createXAttachmentTray(): XAttachmentTray {
  let records: readonly XAttachmentTrayCard[] = [];
  const add = async (files: Iterable<XAttachmentFile>): Promise<readonly XAttachmentTrayCard[]> => {
    const added = await cardsFromFiles(files, records.length);
    records = [...records, ...added];
    return added;
  };
  return { cards: () => records, addFromPicker: add, addFromDrop: add };
}

/** Static markup avoids inline styles, which the X WebView CSP rejects. */
export function xAttachmentTrayMarkup(cards: readonly XAttachmentTrayCard[]): string {
  const rows = cards.map((card) => `<article class="x-attachment-tray-card" data-x-attachment-card="${escapeHtml(card.id)}" data-x-attachment-fingerprint="${card.fingerprint}"><strong>${escapeHtml(card.name)}</strong><small>${card.size.toLocaleString("en-US")} bytes · ${escapeHtml(card.fingerprint)}</small></article>`).join("");
  return `<section class="x-attachment-tray" aria-label="X attachment tray" data-x-attachment-limit-files="${X_ATTACHMENT_MAX_FILES}" data-x-attachment-limit-bytes="${X_ATTACHMENT_MAX_BYTES}"><header><strong>Attachments</strong><label class="button compact" for="x-attachment-picker">Choose file</label><input id="x-attachment-picker" data-x-attachment-picker type="file" multiple></header><p class="x-attachment-tray-limit">Up to ${X_ATTACHMENT_MAX_FILES} files, 8 MB each.</p><div data-x-attachment-drop-target tabindex="0">${rows || "Drop files here or choose files."}</div></section>`;
}

type XTrayEvent = Event & { dataTransfer?: { files: ArrayLike<XAttachmentFile> } | null };
type XTrayTarget = Pick<EventTarget, "addEventListener">;

/**
 * Binds both native paths without accepting a send callback.  Callers rerender
 * after a successful intake; this function cannot send an X message.
 */
export function bindXAttachmentTray(
  picker: XTrayTarget,
  dropTarget: XTrayTarget,
  tray: XAttachmentTray,
  onChanged: () => void,
): void {
  picker.addEventListener("change", (event) => {
    const input = event.currentTarget as HTMLInputElement | null;
    void tray.addFromPicker(Array.from(input?.files ?? [])).then((added) => { if (added.length) onChanged(); });
  });
  dropTarget.addEventListener("dragover", (event) => event.preventDefault());
  dropTarget.addEventListener("drop", (event) => {
    const drop = event as XTrayEvent;
    event.preventDefault();
    void tray.addFromDrop(Array.from(drop.dataTransfer?.files ?? [])).then((added) => { if (added.length) onChanged(); });
  });
}
