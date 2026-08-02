import type { ProviderDeletionReceipt } from "./scrub-delete-engine";
import { executeScrubDryRun, type ScrubDryRunRequest } from "./scrub-engine-host";

export interface ScrubDryRunPreview {
  receipt: ProviderDeletionReceipt;
  markup: string;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** A preview is only valid for the engine's non-mutating dry-run receipt. */
export function renderScrubDryRunPreview(receipt: ProviderDeletionReceipt): string {
  if (!receipt.dryRun) throw new Error("a live deletion receipt cannot be shown as a dry-run preview");

  const items = receipt.items.map((item, index) =>
    `<li><strong>Item ${index + 1}</strong><dl><div><dt>Channel</dt><dd><code>${escapeHtml(item.channelId)}</code></dd></div><div><dt>Item</dt><dd><code>${escapeHtml(item.itemId)}</code></dd></div></dl></li>`,
  ).join("");
  const selection = items
    ? `<ol class="scrub-dryrun-items">${items}</ol>`
    : "<p>No items match this selection. Nothing will be deleted.</p>";

  return `<section class="scrub-dryrun-preview" aria-labelledby="scrub-dryrun-heading"><header><p class="scrub-dryrun-eyebrow">Dry-run preview</p><h2 id="scrub-dryrun-heading">Review what would be deleted</h2><p class="scrub-dryrun-safety" role="status"><strong>Nothing has been deleted.</strong> This list is exactly what Scrub would send to deletion after a later confirmation.</p></header><p class="scrub-dryrun-target">Account <code>${escapeHtml(receipt.accountId)}</code> on <code>${escapeHtml(receipt.providerId)}</code></p>${selection}</section>`;
}

/** Executes the fixed dry-run host before rendering its owner-facing preview. */
export async function createScrubDryRunPreview(request: ScrubDryRunRequest): Promise<ScrubDryRunPreview> {
  const receipt = await executeScrubDryRun(request);
  return { receipt, markup: renderScrubDryRunPreview(receipt) };
}
