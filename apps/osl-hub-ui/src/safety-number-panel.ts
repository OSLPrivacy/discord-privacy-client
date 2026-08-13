import * as QRCode from "qrcode";

/** The sole shipping presentation of a verified pair's safety number (5068). */
export const SAFETY_NUMBER_DIGITS = 60;
export const SAFETY_NUMBER_GROUPS = 12;

/**
 * A QR payload is deliberately the digits only. A normal QR scanner therefore
 * returns exactly the value the person sees, without a UI-specific prefix for
 * another client to strip before it can compare the pair.
 */
export function safetyNumberScannablePayload(safetyNumber: string): string | null {
  const digits = safetyNumber.replaceAll(/\s/g, "");
  return /^\d{60}$/u.test(digits) ? digits : null;
}

export function decodeSafetyNumberScannablePayload(payload: string): string | null {
  return /^\d{60}$/u.test(payload) ? payload : null;
}

function qrModulesSvg(payload: string): string {
  // `qrcode` implements the QR encoding and error correction; this renderer
  // only turns its finished module matrix into SVG. Keeping the payload as one
  // numeric segment makes a phone scan yield these 60 digits verbatim.
  const matrix = QRCode.create(payload, { errorCorrectionLevel: "M" }).modules;
  const quietZone = 4;
  const width = matrix.size + quietZone * 2;
  const modules: string[] = [];
  for (let row = 0; row < matrix.size; row += 1) {
    for (let column = 0; column < matrix.size; column += 1) {
      if (matrix.get(row, column)) modules.push(`M${column + quietZone},${row + quietZone}h1v1h-1z`);
    }
  }
  return `<svg class="safety-number-qr" viewBox="0 0 ${width} ${width}" role="img" aria-label="Scannable verification code" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="white"/><path d="${modules.join("")}" fill="black"/></svg>`;
}

/**
 * Renders the number and QR code from the one backend-supplied value. This
 * function derives neither a fingerprint nor a second display value: 3083 is
 * the only derivation and this panel refuses partial or differently-grouped
 * data rather than silently shortening or reformatting it.
 */
export function safetyNumberPanelMarkup(safetyNumber: string | null): string {
  if (!safetyNumber || !/^(?:\d{5})(?: \d{5}){11}$/u.test(safetyNumber)) {
    return `<p class="verification-code-unavailable" role="status">Verification code unavailable.</p>`;
  }
  const payload = safetyNumberScannablePayload(safetyNumber);
  if (!payload) return `<p class="verification-code-unavailable" role="status">Verification code unavailable.</p>`;
  return `<section class="safety-number-panel" aria-label="Shared verification code for this friend"><code class="verification-code" data-safety-number="${payload}">${safetyNumber}</code><div class="safety-number-scannable" data-safety-number-payload="${payload}">${qrModulesSvg(payload)}</div></section>`;
}
