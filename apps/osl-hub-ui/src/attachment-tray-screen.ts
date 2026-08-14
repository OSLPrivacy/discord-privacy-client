/** Render the named fields held by one pending encrypted attachment. */
export interface AttachmentTrayCard {
  removableId: string;
  name: string;
  type: string;
  size: number;
  previewDataUrl: string | null;
}

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

export function attachmentTraySizeLabel(size: number): string {
  if (size < 1_024) return `${size} B`;
  const units = ["KB", "MB", "GB"] as const;
  let scaled = size / 1_024;
  let unitIndex = 0;
  while (scaled >= 1_024 && unitIndex < units.length - 1) {
    scaled /= 1_024;
    unitIndex += 1;
  }
  return `${scaled.toFixed(scaled >= 10 ? 0 : 1)} ${units[unitIndex]}`;
}

export function attachmentTrayCardMarkup(card: AttachmentTrayCard): string {
  return `<article data-attachment-tray-card="${escapeHtml(card.removableId)}"><strong class="attachment-tray-card__name">${escapeHtml(card.name)}</strong><span class="attachment-tray-card__meta">${escapeHtml(card.type)} · ${attachmentTraySizeLabel(card.size)}</span></article>`;
}

export function attachmentTrayScreenMarkup(cards: readonly AttachmentTrayCard[]): string {
  return `<section class="attachment-tray" aria-label="Attachment tray">${cards.length ? cards.map(attachmentTrayCardMarkup).join("") : "<p>No files in the tray.</p>"}</section>`;
}
