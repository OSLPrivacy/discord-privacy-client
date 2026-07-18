import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const sheetStyles = readFileSync(new URL("./local-protected-sheet.css", import.meta.url), "utf8");
const overlayStyles = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");
const allStyles = `${styles}\n${sheetStyles}\n${overlayStyles}`;

describe("professional typography", () => {
  it("uses a native Windows UI family and reserves mono for machine data", () => {
    expect(styles).toContain('--font-ui: "Segoe UI Variable Text"');
    expect(styles).toContain('--font-display: "Segoe UI Variable Display"');
    expect(styles).toContain('--font-mono: "Cascadia Mono"');
    expect(styles).toContain("font-family: var(--font-ui)");
    expect(styles).toContain(".identity-row small { font-family: var(--font-mono); }");
    expect(sheetStyles).toContain(".local-capsule-result textarea { font-family: var(--font-mono)");
  });

  it("does not regress to novelty fonts, tiny text, or fractional weights", () => {
    expect(allStyles).not.toMatch(/Px437|Bahnschrift/);
    expect(allStyles).not.toMatch(/font-size:\s*(?:[1-9]|10)px/);
    expect(allStyles).not.toMatch(/font-weight:\s*(?:350|650|680|720|750|800)/);
  });
});
