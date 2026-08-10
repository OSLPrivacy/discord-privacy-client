import { describe, expect, it } from "vitest";
import { safetyNumberPanelMarkup } from "./safety-number-panel";

const numberOf = (length: number) => "7".repeat(length);

describe("safety number panel length gate", () => {
  it("refuses 59 and 61 digits by naming the required length", () => {
    for (const length of [59, 61]) {
      expect(() => safetyNumberPanelMarkup({
        id: `wrong-${length}`,
        name: "Wrong Length",
        safetyNumber: numberOf(length),
        verified: false,
      })).toThrow("A safety number must contain exactly 60 digits.");
      console.log(`TASK5068 refused length=${length} error=A safety number must contain exactly 60 digits.`);
    }
  });

  it("catches a throwaway renderer that would display 59 digits", () => {
    const short = numberOf(59);
    const throwawayRenderer = (value: string) => `<div class="number">${value}</div>`;
    const rendered = throwawayRenderer(short);
    expect(rendered).toContain(short);
    expect(() => safetyNumberPanelMarkup({
      id: "throwaway-59",
      name: "Throwaway",
      safetyNumber: short,
      verified: false,
    })).toThrow("exactly 60 digits");
    console.log(`TASK5068 throwaway_render_59=true guarded=true`);
  });
});
