import type { ItemDeletionReceipt, ProviderDeletionReceipt } from "./scrub-delete-engine";

export interface ScrubReceiptCounts {
  deleted: number;
  stillPresent: number;
  unknown: number;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Unknown is its own terminal outcome: it is never included in `deleted`. */
export function scrubReceiptCounts(items: readonly ItemDeletionReceipt[]): ScrubReceiptCounts {
  return items.reduce<ScrubReceiptCounts>((counts, item) => {
    if (item.outcome === "confirmed-deleted") counts.deleted += 1;
    if (item.outcome === "confirmed-not-deleted") counts.stillPresent += 1;
    if (item.outcome === "UNKNOWN") {
      counts.unknown += 1;
    }
    return counts;
  }, { deleted: 0, stillPresent: 0, unknown: 0 });
}

function receiptRow(item: ItemDeletionReceipt, index: number): string {
  if (item.outcome === "confirmed-deleted") {
    return `<li class="scrub-receipt-row deleted"><strong>Item ${index + 1}: deleted</strong><p>${escapeHtml(item.detail)}</p></li>`;
  }
  if (item.outcome === "confirmed-not-deleted") {
    return `<li class="scrub-receipt-row still-present"><strong>Item ${index + 1}: still present</strong><p>${escapeHtml(item.detail)}</p></li>`;
  }
  return `<li class="scrub-receipt-row unknown"><strong>Item ${index + 1}: Unknown</strong><p>${escapeHtml(item.detail)}</p><a href="#scrub-manual-resolution">Resolve manually</a></li>`;
}

/**
 * Renders only the run result: source identifiers remain out of the receipt.
 * The manual-resolution anchor is wired by the containing Scrub surface.
 */
export function renderScrubReceipt(receipt: ProviderDeletionReceipt): string {
  const counts = scrubReceiptCounts(receipt.items);
  const rows = receipt.items.map(receiptRow).join("");
  return `<section class="scrub-receipt" aria-live="polite"><h3>Cleanup receipt</h3><div class="scrub-receipt-counts" aria-label="Cleanup outcome counts"><p class="scrub-receipt-count deleted" aria-label="${counts.deleted} deleted"><strong>${counts.deleted}</strong> deleted</p><p class="scrub-receipt-count still-present" aria-label="${counts.stillPresent} still present"><strong>${counts.stillPresent}</strong> still present</p><p class="scrub-receipt-count unknown" aria-label="${counts.unknown} Unknown"><strong>${counts.unknown}</strong> Unknown</p></div>${rows ? `<ol class="scrub-receipt-items">${rows}</ol>` : "<p>No items were selected for this run.</p>"}</section>`;
}
