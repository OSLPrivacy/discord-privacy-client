import { describe, expect, it } from "vitest";
import {
  imageComparisonQualityResult,
  imageComparisonQualityText,
  imageComparisonScreenMarkup,
  type ImageComparisonScreenModel,
} from "./image-comparison-screen";

function model(overrides: Partial<ImageComparisonScreenModel> = {}): ImageComparisonScreenModel {
  return {
    originalSrc: "blob:osl/private-original",
    preparedSrc: "blob:osl/prepared-post-copy",
    qualityPassed: true,
    pointerHex: "0668066806680668066806680668066806680668",
    ...overrides,
  };
}

describe("TASK0668 image comparison screen", () => {
  it("draws both pictures side by side with their exact captions", () => {
    const markup = imageComparisonScreenMarkup(model());
    expect(markup).toContain('<h1 id="route-heading" tabindex="-1">Image comparison</h1>');
    expect(markup).toContain('data-image-comparison-side="original"');
    expect(markup).toContain('data-image-comparison-side="prepared"');
    expect(markup).toContain('<img alt="Private original" src="blob:osl/private-original"/>');
    expect(markup).toContain('<img alt="Prepared post copy" src="blob:osl/prepared-post-copy"/>');
    expect(markup).toContain("<figcaption>Private original</figcaption>");
    expect(markup).toContain("<figcaption>Prepared post copy</figcaption>");
    const originalIndex = markup.indexOf('data-image-comparison-side="original"');
    const preparedIndex = markup.indexOf('data-image-comparison-side="prepared"');
    expect(originalIndex).toBeGreaterThan(-1);
    expect(preparedIndex).toBeGreaterThan(originalIndex);
  });

  it("shows a passed quality result with the recovered pointer", () => {
    const markup = imageComparisonScreenMarkup(model());
    expect(imageComparisonQualityResult({ qualityPassed: true })).toBe("passed");
    expect(imageComparisonQualityText({ qualityPassed: true })).toBe("Quality check passed");
    expect(markup).toContain('data-quality-result="passed"');
    expect(markup).toContain("Quality check passed");
    expect(markup).toContain("Recovered pointer 0668066806680668066806680668066806680668");
  });

  it("shows a failed quality result without a recovered pointer", () => {
    const markup = imageComparisonScreenMarkup(model({ qualityPassed: false }));
    expect(imageComparisonQualityResult({ qualityPassed: false })).toBe("failed");
    expect(markup).toContain('data-quality-result="failed"');
    expect(markup).toContain("Quality check failed");
    expect(markup).not.toContain("Recovered pointer");
  });

  it("escapes attacker-controlled sources and pointer text", () => {
    const markup = imageComparisonScreenMarkup(
      model({
        originalSrc: 'blob:"/><script>alert(1)</script>',
        pointerHex: "<img onerror=x>",
      }),
    );
    expect(markup).not.toContain("<script>");
    expect(markup).not.toContain("<img onerror");
    expect(markup).toContain("&lt;script&gt;");
  });
});
