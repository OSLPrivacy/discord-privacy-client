/**
 * TASK 0625 - attachment tray screen.
 *
 * Pure markup for the tray populated by TASK 0621's
 * `AttachmentTrayRecord { name, type, size, removableId }`. One card per
 * file: name, type, size, a preview for pictures, and a remove button.
 */
export interface AttachmentTrayCard {
  removableId: string;
  name: string;
  type: string;
  size: number;
  /** Only set for `isAttachmentTrayPicture(type)`; omitted otherwise. */
  previewDataUrl: string | null;
}

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

export function isAttachmentTrayPicture(type: string): boolean {
  return /^image\/[a-z0-9.+-]+$/iu.test(type);
}
export function attachmentTraySizeLabel(size: number): string {
  if (size < 1_024) return `${size} B`;
  const units = ["KB", "MB", "GB"] as const;
  let scaled = size / 1_024;
  let unitIndex = 0;
  while (scaled >= 1_024 && unitIndex < units.length - 1) {
    scaled /= 1_024;
    unitIndex += 1;
  }
  const digits = scaled >= 10 ? 0 : 1;
  return `${scaled.toFixed(digits)} ${units[unitIndex]}`;
}

export function attachmentTrayCardMarkup(card: AttachmentTrayCard): string {
  const isPicture = isAttachmentTrayPicture(card.type);
  const preview = isPicture && card.previewDataUrl
    ? `<img class="attachment-tray-card__preview" src="${escapeHtml(card.previewDataUrl)}" alt="Preview of ${escapeHtml(card.name)}" />`
    : `<span class="attachment-tray-card__preview attachment-tray-card__preview--none" aria-hidden="true"></span>`;
  return `<article class="attachment-tray-card" data-attachment-tray-card="${escapeHtml(card.removableId)}" data-attachment-tray-kind="${isPicture ? "picture" : "document"}">${preview}<div class="attachment-tray-card__body"><strong class="attachment-tray-card__name">${escapeHtml(card.name)}</strong><span class="attachment-tray-card__meta">${escapeHtml(card.type)} · ${attachmentTraySizeLabel(card.size)}</span></div><button class="attachment-tray-card__remove" type="button" data-attachment-tray-remove="${escapeHtml(card.removableId)}" aria-label="Remove ${escapeHtml(card.name)}">Remove</button></article>`;
}

export function attachmentTrayScreenMarkup(cards: readonly AttachmentTrayCard[]): string {
  const body = cards.length
    ? `<div class="attachment-tray-grid">${cards.map(attachmentTrayCardMarkup).join("")}</div>`
    : `<p class="attachment-tray-empty">No files in the tray.</p>`;
  return `<section class="attachment-tray" aria-label="Attachment tray"><h2 class="attachment-tray__title">Attachment tray</h2>${body}</section>`;
}

/** Wires the remove buttons rendered by `attachmentTrayScreenMarkup` under `root`. */
export function bindAttachmentTrayScreen(root: ParentNode, onRemove: (removableId: string) => void): void {
  root.querySelectorAll<HTMLButtonElement>("[data-attachment-tray-remove]").forEach((button) => {
    button.addEventListener("click", () => {
      const removableId = button.dataset.attachmentTrayRemove;
      if (removableId) onRemove(removableId);
    });
  });
}
