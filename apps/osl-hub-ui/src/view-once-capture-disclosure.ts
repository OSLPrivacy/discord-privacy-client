/**
 * TASK 6844 - what OSL tells the viewer and the sender about screenshot
 * detection, before either of them uses view-once.
 *
 * These two strings are byte-identical to `CAPTURE_DISCLOSURE_VIEWER` and
 * `CAPTURE_DISCLOSURE_SENDER` in `crates/view-once-capture/src/disclosure.rs`,
 * and `view-once-capture-disclosure.test.ts` reads that Rust file and fails if
 * they ever drift.
 *
 * The list of phrases that would over-promise lives in `disclosure.rs` and is
 * asserted from there by the test. It is deliberately not repeated in shipped
 * app source: `scripts/check-app-claims.mjs` scans every string literal under
 * `apps/osl-hub-ui/src`, and a file that spells out banned phrases in order to
 * ban them is indistinguishable, to a string gate, from one that makes them.
 *
 * They are fixed strings rather than screen words on purpose. Every other
 * caption here can be reworded by a translator without changing what the
 * product does; this one is a statement about the limits of a security
 * feature, and a translation that quietly drops "OSL does not stop
 * screenshots" turns an honest disclosure into a false promise. A translated
 * variant has to pass the same honesty check the English does before it can
 * ship.
 */

/** Shown on the view-once viewer before the content is revealed. */
export const CAPTURE_DISCLOSURE_VIEWER =
  "Before you open this: OSL can detect only the screen-capture paths Windows reports to it — the PrintScreen key and the Windows snip, which put a picture on the clipboard. If OSL detects one of those while this is open, the sender is told once. OSL cannot detect a camera pointed at your screen, an external capture device, or every capture tool, and OSL does not stop screenshots.";

/** Shown on the composer before view-once is sent. */
export const CAPTURE_DISCLOSURE_SENDER =
  "Before you send this: OSL can detect only the screen-capture paths Windows reports to it — the PrintScreen key and the Windows snip, which put a picture on the clipboard. You are told once if OSL detects one of those. OSL cannot detect a camera pointed at their screen, an external capture device, or every capture tool, and OSL does not stop screenshots. No notification does not mean no copy was made.";

/** The capture paths OSL cannot see, each of which the copy above must name. */
export const UNSUPPORTED_CAPTURE_PATHS: readonly string[] = [
  "a camera pointed at",
  "an external capture device",
  "every capture tool",
];

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (char) => (
    { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char] as string
  ));
}

/** The viewer's disclosure, rendered above the control that reveals content. */
export function viewerCaptureDisclosureMarkup(): string {
  return `<p class="voo-capture-disclosure" data-voo-capture-disclosure>${escapeHtml(CAPTURE_DISCLOSURE_VIEWER)}</p>`;
}

/** The sender's disclosure, rendered inside the composer's view-once control. */
export function senderCaptureDisclosureMarkup(): string {
  return `<small class="osl-chat-capture-disclosure" data-osl-chat-capture-disclosure>${escapeHtml(CAPTURE_DISCLOSURE_SENDER)}</small>`;
}
