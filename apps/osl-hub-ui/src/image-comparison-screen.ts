/**
 * The image comparison screen: the private original next to the prepared
 * post copy, with the image quality result from the 0660/0664/0665 chain.
 *
 * The Hub prepares the post copy and runs the quality check (the hidden
 * pointer must read back from the prepared copy). This module only draws the
 * screen from a result the caller already holds, so a fixture can render it
 * without a backend. The private original never leaves the device; only the
 * prepared copy is ever posted.
 */

export type ImageComparisonQualityResult = "passed" | "failed";

export interface ImageComparisonScreenModel {
  /** Local object URL of the private original. Never uploaded. */
  originalSrc: string;
  /** Local object URL of the prepared post copy. */
  preparedSrc: string;
  /** True when the hidden pointer read back from the prepared copy. */
  qualityPassed: boolean;
  /** Recovered pointer hex, shown only when the quality check passed. */
  pointerHex?: string;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function imageComparisonQualityResult(
  model: Pick<ImageComparisonScreenModel, "qualityPassed">,
): ImageComparisonQualityResult {
  return model.qualityPassed ? "passed" : "failed";
}

export function imageComparisonQualityText(
  model: Pick<ImageComparisonScreenModel, "qualityPassed">,
): string {
  return model.qualityPassed ? "Quality check passed" : "Quality check failed";
}

export function imageComparisonScreenMarkup(model: ImageComparisonScreenModel): string {
  const result = imageComparisonQualityResult(model);
  const pointer = model.qualityPassed && model.pointerHex
    ? `<small class="image-comparison-pointer">Recovered pointer ${escapeHtml(model.pointerHex)}</small>`
    : "";
  return `<section class="image-comparison-screen" aria-labelledby="route-heading">
  <h1 id="route-heading" tabindex="-1">Image comparison</h1>
  <div class="image-comparison-pair">
    <figure class="image-comparison-figure" data-image-comparison-side="original">
      <img alt="Private original" src="${escapeHtml(model.originalSrc)}"/>
      <figcaption>Private original</figcaption>
    </figure>
    <figure class="image-comparison-figure" data-image-comparison-side="prepared">
      <img alt="Prepared post copy" src="${escapeHtml(model.preparedSrc)}"/>
      <figcaption>Prepared post copy</figcaption>
    </figure>
  </div>
  <p class="image-comparison-quality" role="status" data-quality-result="${result}">${imageComparisonQualityText(model)}${pointer}</p>
</section>`;
}
