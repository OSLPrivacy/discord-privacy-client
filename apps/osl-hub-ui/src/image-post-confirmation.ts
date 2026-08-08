/**
 * TASK 0669 — connect image post confirmation.
 *
 * The 0660/0661 chain prepares the post copy and 0664/0665 runs the image
 * quality check (the hidden pointer must read back from the prepared copy).
 * This module connects that result to the post control: the post stays
 * disabled — with the reason — until quality passes, and confirmation sends
 * only the prepared copy. The private original is never postable.
 *
 * The reported `selectedCopyId` is taken from the request that was actually
 * handed to the provider post callback, so state and send cannot disagree.
 */

export const IMAGE_POST_QUALITY_FAILED_REASON =
  "Quality check failed: the hidden pointer did not read back from the prepared copy";
export const IMAGE_POST_MISSING_COPY_REASON =
  "No prepared post copy exists yet";
export const IMAGE_POST_ORIGINAL_SELECTED_REASON =
  "The prepared copy is the private original; refusing to post it";

export interface ImagePostFixture {
  /** Id of the private original. Never leaves the device. */
  originalId: string;
  /** Id of the prepared post copy from the image-hiding chain. */
  preparedCopyId: string;
  /** True when the 0664/0665 quality read-back succeeded. */
  qualityPassed: boolean;
}

/** The only payload confirmation may produce: the prepared copy, nothing else. */
export interface ImagePostRequest {
  copyId: string;
}

export type ImagePostConfirmationState =
  | { post: "disabled"; reason: string }
  | { post: "ready-to-confirm" }
  | { post: "confirmed"; selectedCopyId: string };

export interface ImagePostConfirmationControl {
  state(): ImagePostConfirmationState;
  confirm(): ImagePostConfirmationState;
}

export function imagePostDisabledReason(fixture: ImagePostFixture): string | undefined {
  if (!fixture.qualityPassed) return IMAGE_POST_QUALITY_FAILED_REASON;
  if (fixture.preparedCopyId === "") return IMAGE_POST_MISSING_COPY_REASON;
  if (fixture.preparedCopyId === fixture.originalId) return IMAGE_POST_ORIGINAL_SELECTED_REASON;
  return undefined;
}

/**
 * Connects a prepared-copy fixture to the provider post callback. While any
 * disabled reason holds, `confirm()` sends nothing. Once confirmed, repeat
 * confirmations do not post again.
 */
export function connectImagePostConfirmation(
  fixture: ImagePostFixture,
  post: (request: ImagePostRequest) => void,
): ImagePostConfirmationControl {
  let sent: ImagePostRequest | undefined;

  const state = (): ImagePostConfirmationState => {
    const reason = imagePostDisabledReason(fixture);
    if (reason !== undefined) return { post: "disabled", reason };
    if (sent !== undefined) return { post: "confirmed", selectedCopyId: sent.copyId };
    return { post: "ready-to-confirm" };
  };

  const confirm = (): ImagePostConfirmationState => {
    if (state().post !== "ready-to-confirm") return state();
    const request: ImagePostRequest = { copyId: fixture.preparedCopyId };
    post(request);
    sent = request;
    return state();
  };

  return { state, confirm };
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

export function imagePostConfirmationMarkup(state: ImagePostConfirmationState): string {
  const label = state.post === "confirmed" ? "Posted prepared copy" : "Post prepared copy";
  const disabled = state.post === "disabled";
  const attributes = [
    `class="image-post-confirm"`,
    `type="button"`,
    `data-image-post="${state.post}"`,
  ];
  if (disabled) attributes.push(`disabled`, `data-disabled-reason="${escapeAttribute(state.reason)}"`);
  if (state.post === "confirmed") {
    attributes.push(`data-selected-copy-id="${escapeAttribute(state.selectedCopyId)}"`);
  }
  const status = disabled
    ? state.reason
    : state.post === "confirmed"
      ? `Sent prepared copy ${state.selectedCopyId}`
      : "Quality check passed; confirm to post the prepared copy";
  return `<div class="image-post-confirmation"><button ${attributes.join(" ")}>${label}</button><p class="image-post-status" role="status">${escapeAttribute(status)}</p></div>`;
}

/** The two fixtures differ only in the quality result; ids are shared. */
export function failedQualityImagePostFixture(): ImagePostFixture {
  return {
    originalId: "image-original-06691020304050607080",
    preparedCopyId: "image-copy-0669f00dfeedface0669",
    qualityPassed: false,
  };
}

export function passingQualityImagePostFixture(): ImagePostFixture {
  return {
    originalId: "image-original-06691020304050607080",
    preparedCopyId: "image-copy-0669f00dfeedface0669",
    qualityPassed: true,
  };
}
