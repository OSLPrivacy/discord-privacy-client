import { describe, expect, it } from "vitest";
import { applyThemeMod, parseThemeMod } from "./theme-mod";

const valid = JSON.stringify({
  version: 1,
  name: "Ocean",
  colors: { brand: "#00bddd", background: "#080b0d", panel: "#11171a", text: "#f0f8fa", muted: "#91a2a8" },
  radius: 5,
});

describe("theme mods", () => {
  it("accepts a bounded data-only theme", () => {
    expect(parseThemeMod(valid)?.name).toBe("Ocean");
  });

  it("rejects executable, partial, and unsafe CSS values", () => {
    expect(parseThemeMod(JSON.stringify({ version: 1, name: "x", colors: {}, radius: 2 }))).toBeNull();
    expect(parseThemeMod(valid.replace("#00bddd", "url(javascript:alert(1))"))).toBeNull();
    expect(parseThemeMod(valid.replace('"radius":5', '"radius":99'))).toBeNull();
    expect(parseThemeMod(valid.replace('"radius":5', '"radius":5,"script":"x"'))).toBeNull();
  });

  it("applies and fully removes the whitelisted variables", () => {
    const values = new Map<string, string>();
    const root = {
      dataset: {},
      style: {
        setProperty: (key: string, value: string) => { values.set(key, value); },
        removeProperty: (key: string) => values.delete(key) ? "removed" : "",
        getPropertyValue: (key: string) => values.get(key) ?? "",
      },
    } as unknown as HTMLElement;
    const mod = parseThemeMod(valid);
    applyThemeMod(root, mod);
    expect(root.style.getPropertyValue("--brand")).toBe("#00bddd");
    expect(root.dataset.themeMod).toBe("Ocean");
    applyThemeMod(root, null);
    expect(root.style.getPropertyValue("--brand")).toBe("");
    expect(root.dataset.themeMod).toBeUndefined();
  });
});
