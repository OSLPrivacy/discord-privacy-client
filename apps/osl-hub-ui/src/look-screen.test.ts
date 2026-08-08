import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { applyLookState, defaultLookState, lookScreenMarkup, parseLookState } from "./look-screen";

describe("Look screen", () => {
  it("renders every requested local look control with a one-line explanation", () => {
    const markup = lookScreenMarkup(defaultLookState);
    for (const text of [
      "Light", "Dark", "Computer", "Named looks", "Midnight", "Paper", "Signal",
      "Accent", "Corners and glow", "Text and spacing", "Reset look",
    ]) expect(markup).toContain(text);
    for (const control of ["mode", "named", "accent", "corners", "glow", "text", "spacing"]) {
      expect(markup).toContain(`data-look-${control}=`);
    }
    expect(markup).toContain("data-look-reset");
    expect(markup.match(/<small>/gu)?.length).toBeGreaterThanOrEqual(18);
  });

  it("is the live Settings Look section and binds every control", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(main).toContain('[["account", "Account"], ["apps", "Apps"], ["scrub", "Scrub"], ["cleanup", "Cleanup"], ["notifications", "Notifications"], ["appearance", "Look"]');
    expect(main).toContain("return lookScreenMarkup(lookState);");
    for (const control of ["mode", "named", "accent", "corners", "glow", "text", "spacing", "reset"]) {
      expect(main).toContain(`data-look-${control}`);
    }
  });

  it("rejects malformed preferences and applies only the owned CSS variables", () => {
    expect(parseLookState('{"mode":"neon"}')).toEqual(defaultLookState);
    const values = new Map<string, string>();
    const root = {
      dataset: {},
      style: {
        setProperty: (key: string, value: string) => { values.set(key, value); },
        removeProperty: (key: string) => values.delete(key) ? "removed" : "",
      },
    } as unknown as HTMLElement;
    applyLookState(root, { ...defaultLookState, named: "midnight", accent: "violet", corners: "soft", glow: true, text: "large", spacing: "relaxed" });
    expect(values.get("--look-accent")).toBe("#8b5cf6");
    expect(root.dataset.lookNamed).toBe("midnight");
    expect(root.dataset.lookGlow).toBe("true");
  });
});
