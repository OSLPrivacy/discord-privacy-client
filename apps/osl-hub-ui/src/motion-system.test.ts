import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

describe("restrained motion system", () => {
  it("enters only when the navigation key changes", () => {
    expect(source).toContain('if (focusKey !== lastFocusKey)');
    expect(source).toContain('classList.add("view-enter")');
    expect(source).toContain('`${route}:${activeService?.id ?? "none"}:${serviceGuideStep ?? "app"}`');
    expect(styles).toMatch(/\.view-enter\s*\{[\s\S]*?animation:\s*view-enter var\(--motion-slow\)/);
  });

  it("uses one brief transition for tool routes and modal tools", () => {
    expect(source).toContain('if (route === "settings" || route === "service") view?.classList.add("tool-enter")');
    expect(styles).toMatch(/\.tool-enter\s*\{[^}]*animation:\s*tool-enter var\(--motion-base\)/s);
    expect(styles).toMatch(/\.unlock-dialog\[open\],[\s\S]*?\.scrub-review-dialog\[open\],[\s\S]*?animation:\s*tool-enter var\(--motion-base\)/);
    expect(styles).toContain("--motion-base: 200ms");
  });

  it("opens with one restrained vector-logo reveal", () => {
    expect(source).toContain('class="loading-seal"');
    expect(source).toContain('src="${oslVectorLogoUrl}"');
    expect(styles).toMatch(/\.loading-logo\s*\{[^}]*animation:\s*logo-soft-enter 360ms/s);
    expect(source).not.toContain("security-motion");
  });

  it("uses the same simple one-shot reveal for password unlock", () => {
    expect(source).toContain('class="unlock-logo-stage"');
    expect(source).toMatch(/class="unlock-logo-stage"[\s\S]*?src="\$\{oslVectorLogoUrl\}"[\s\S]*?Enter your password/);
    expect(styles).toMatch(/\.signin-logo\s*\{[^}]*animation:\s*signin-logo-reveal 440ms/s);
    expect(styles).toMatch(/\.unlock-logo-stage \.osl-logo\s*\{[^}]*animation:\s*logo-soft-enter 360ms/s);
    expect(styles).not.toMatch(/security-(?:center|key|shackle|body|lock)/);
  });

  it("limits interaction motion to compositor-friendly properties", () => {
    expect(styles).toContain("--motion-fast: 160ms");
    expect(styles).toContain("--motion-slow: 240ms");
    expect(styles).toMatch(/\.app-logo-plate[\s\S]*?transition:[^;]*transform/);
    expect(styles).not.toMatch(/transition:\s*(?:all|width|height|inset|padding|margin)/);
  });

  it("keeps loading and the explicit comparison as the only repeating motion", () => {
    const infiniteAnimations = [...styles.matchAll(/animation:\s*([^;]*\binfinite\b[^;]*);/g)].map((match) => match[1]);
    expect(infiniteAnimations.length).toBeGreaterThan(0);
    expect(infiniteAnimations.every((animation) => /loading-line|placement-|demo-pulse/.test(animation))).toBe(true);
  });

  it("provides complete static states for reduced motion", () => {
    const reduced = styles.slice(styles.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(reduced).toContain("animation-iteration-count: 1 !important");
    expect(reduced).toContain(".placement-demo-typing { width: 17ch !important; }");
    expect(reduced).toContain(".placement-demo article::after");
    expect(reduced).toContain(".view-enter");
    expect(reduced).toContain(".tool-enter");
    expect(reduced).toContain(".signin-logo");
    expect(reduced).toContain(".unlock-logo-stage .osl-logo");
    expect(reduced).toContain('.toast { transform: translateX(-50%) !important; }');
  });

  it("gives transient feedback an exit instead of abruptly removing it", () => {
    expect(source).toContain('toast.classList.add("toast-leaving")');
    expect(source).toContain('toast.addEventListener("animationend"');
    expect(styles).toContain("@keyframes toast-exit");
  });
});
