/**
 * TASK 1049 — Signal attachment intake.
 *
 * Selection and dropping are deliberately only local staging operations. A
 * card means that OSL accepted the file for a later explicit send; neither
 * path has a send callback or reaches Signal's composer.
 */
export const SIGNAL_ATTACHMENT_MAX_FILES = 16;
export const SIGNAL_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;

export interface SignalAttachmentFile {
  readonly name: string;
  readonly type?: string;
  readonly size: number;
}

export interface SignalAttachmentTrayCard {
  readonly id: string;
  readonly name: string;
  readonly type: string;
  readonly size: number;
  readonly state: "unsent";
}

export interface SignalAttachmentTray {
  cards(): readonly SignalAttachmentTrayCard[];
  addFromPicker(files: Iterable<SignalAttachmentFile>): readonly SignalAttachmentTrayCard[];
  addFromDrop(files: Iterable<SignalAttachmentFile>): readonly SignalAttachmentTrayCard[];
}

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

function validFile(file: SignalAttachmentFile): boolean {
  return file.name.trim().length > 0
    && Number.isSafeInteger(file.size)
    && file.size > 0
    && file.size <= SIGNAL_ATTACHMENT_MAX_BYTES;
}

/** Creates a local-only Signal tray with one admission reducer for both inputs. */
export function createSignalAttachmentTray(): SignalAttachmentTray {
  let records: readonly SignalAttachmentTrayCard[] = [];
  const add = (files: Iterable<SignalAttachmentFile>): readonly SignalAttachmentTrayCard[] => {
    const accepted = [...files].filter(validFile);
    if (records.length + accepted.length > SIGNAL_ATTACHMENT_MAX_FILES) return [];
    const added = accepted.map((file, index) => ({
      id: `signal-attachment-${records.length + index + 1}`,
      name: file.name.trim(),
      type: file.type?.trim() || "application/octet-stream",
      size: file.size,
      state: "unsent" as const,
    }));
    records = [...records, ...added];
    return added;
  };
  return { cards: () => records, addFromPicker: add, addFromDrop: add };
}

/** Markup placed inside the Signal private-box surface. */
export function signalAttachmentTrayMarkup(cards: readonly SignalAttachmentTrayCard[]): string {
  const rows = cards.map((card) => `<article class="signal-attachment-tray-card" data-signal-attachment-card="${escapeHtml(card.id)}" data-signal-attachment-state="unsent"><strong>${escapeHtml(card.name)}</strong><small>${card.size.toLocaleString("en-US")} bytes · Unsent</small></article>`).join("");
  return `<section class="signal-attachment-tray" aria-label="Signal attachment tray" data-signal-attachment-limit-files="${SIGNAL_ATTACHMENT_MAX_FILES}" data-signal-attachment-limit-bytes="${SIGNAL_ATTACHMENT_MAX_BYTES}"><header><strong>Attachments</strong><label for="signal-attachment-picker">Choose file</label><input id="signal-attachment-picker" data-signal-attachment-picker type="file" multiple></header><p>Up to ${SIGNAL_ATTACHMENT_MAX_FILES} files, 8 MB each.</p><div data-signal-attachment-drop-target tabindex="0">${rows || "Drop files here or choose files."}</div></section>`;
}

type SignalTrayEvent = Event & { dataTransfer?: { files: ArrayLike<SignalAttachmentFile> } | null };
type SignalTrayTarget = Pick<EventTarget, "addEventListener">;

/** Binds picker and drop target to intake only; this function cannot send. */
export function bindSignalAttachmentTray(
  picker: SignalTrayTarget,
  dropTarget: SignalTrayTarget,
  tray: SignalAttachmentTray,
  onChanged: () => void,
): void {
  picker.addEventListener("change", (event) => {
    const input = event.currentTarget as HTMLInputElement | null;
    if (tray.addFromPicker(Array.from(input?.files ?? [])).length) onChanged();
  });
  dropTarget.addEventListener("dragover", (event) => event.preventDefault());
  dropTarget.addEventListener("drop", (event) => {
    const drop = event as SignalTrayEvent;
    event.preventDefault();
    if (tray.addFromDrop(Array.from(drop.dataTransfer?.files ?? [])).length) onChanged();
  });
}
