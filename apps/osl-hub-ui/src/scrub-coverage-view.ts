import type { ScrubCoverageReceipt } from "./scrub-plan";

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function messages(count: number): string {
  return count === 1 ? "1 message" : `${count} messages`;
}

function coverageGaps(receipt: ScrubCoverageReceipt): readonly string[] {
  if (receipt.providerReportedComplete) return [];
  return receipt.gaps.length > 0
    ? receipt.gaps
    : ["The provider did not attest that Scrub saw the whole account history."];
}

function checkedContent(receipt: ScrubCoverageReceipt): string {
  const checked = [receipt.textChecked ? "text" : "no text"];
  checked.push(receipt.imagesChecked ? "images" : "no images");
  if (receipt.videosChecked !== undefined) checked.push(receipt.videosChecked ? "videos" : "no videos");
  if (receipt.attachmentsScanned !== undefined) checked.push(`${receipt.attachmentsScanned} attachments`);
  return checked.join(", ");
}

/**
 * A receipt is evidence of a bounded observation, never a claim about the
 * account beyond what the provider attested. In particular, the message count
 * remains an inspected count when the provider could not supply everything.
 */
export function scrubCoverageReceiptMarkup(receipt: ScrubCoverageReceipt): string {
  const gaps = coverageGaps(receipt);
  const incomplete = gaps.length > 0;
  const status = incomplete
    ? `<p class="scrub-coverage-warning" role="status"><strong>We did not see everything.</strong> We inspected ${messages(receipt.messagesScanned)}, not a complete account history.</p><h3>What may be missing</h3><ul>${gaps.map((gap) => `<li>${escapeHtml(gap)}</li>`).join("")}</ul>`
    : `<p role="status"><strong>The provider reported this coverage as complete.</strong> We inspected ${messages(receipt.messagesScanned)}.</p>`;
  return `<section class="scrub-coverage-receipt" aria-labelledby="scrub-coverage-heading"><h2 id="scrub-coverage-heading">What Scrub saw</h2>${status}<dl><div><dt>Target</dt><dd>${escapeHtml(receipt.targetId)}</dd></div><div><dt>Content checked</dt><dd>${escapeHtml(checkedContent(receipt))}</dd></div></dl></section>`;
}
