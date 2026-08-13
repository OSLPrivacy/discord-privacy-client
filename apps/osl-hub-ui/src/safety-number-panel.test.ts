import { describe, expect, it } from "vitest";
import * as QRCode from "qrcode";
import {
  decodeSafetyNumberScannablePayload,
  safetyNumberPanelMarkup,
  safetyNumberScannablePayload,
} from "./safety-number-panel";

const SAFETY_NUMBER = "01234 56789 01234 56789 01234 56789 01234 56789 01234 56789 01234 56789";
const DIGITS = SAFETY_NUMBER.replaceAll(" ", "");

describe("shipping safety-number panel (5068)", () => {
  it("renders exactly the complete 60-digit 3083 value in twelve groups of five", () => {
    const markup = safetyNumberPanelMarkup(SAFETY_NUMBER);
    expect(markup).toContain(SAFETY_NUMBER);
    const displayed = markup.match(/<code class="verification-code"[^>]*>([^<]+)<\/code>/u)?.[1] ?? "";
    expect(displayed.split(" ")).toHaveLength(12);
    expect(displayed.replaceAll(" ", "")).toHaveLength(60);
    expect(markup).toContain(`data-safety-number="${DIGITS}"`);
    expect(markup).toContain("safety-number-qr");
  });

  it("encodes and decodes exactly the rendered digits, with no dialog-owned copy", () => {
    const payload = safetyNumberScannablePayload(SAFETY_NUMBER);
    expect(payload).toBe(DIGITS);
    expect(decodeSafetyNumberScannablePayload(payload ?? "")).toBe(DIGITS);

    const qr = QRCode.create(payload ?? "", { errorCorrectionLevel: "M" });
    expect(qr.segments).toHaveLength(1);
    expect(qr.segments[0]).toMatchObject({ data: DIGITS });
  });

  it("fails closed for truncated, regrouped, or absent values", () => {
    expect(safetyNumberPanelMarkup("01234 56789")).toContain("unavailable");
    expect(safetyNumberPanelMarkup(DIGITS)).toContain("unavailable");
    expect(safetyNumberPanelMarkup(null)).toContain("unavailable");
  });
});
