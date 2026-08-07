import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
// The cover-insertion comparison was restyled out of main.ts / styles.css into
// its own module, so its motion rules are read from there.
const coverSource = readFileSync(new URL("./onboarding-cover.ts", import.meta.url), "utf8");
const coverStyles = readFileSync(new URL("./onboarding-cover.css", import.meta.url), "utf8");

describe("restrained motion system", () => {
  it("enters only when the navigation key changes", () => {
    expect(source).toContain('if (focusKey !== lastFocusKey)');
    expect(source).toContain("const userHasFocusedControl = active instanceof HTMLElement");
    expect(source).toContain("if (!userHasFocusedControl)");
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

  // Protects: the unlock screen reveals the SAME vector mark the loading screen
  // does, once, above its heading -- one shared reveal, not a second bespoke
  // animation. The heading became "Unlock" on 2026-08-06; the mark, the stage
  // it sits in and its one-shot reveal are unchanged and are what this checks.
  it("uses the same simple one-shot reveal for password unlock", () => {
    expect(source).toContain('class="unlock-logo-stage"');
    expect(source).toMatch(/class="unlock-logo-stage"[\s\S]*?src="\$\{oslVectorLogoUrl\}"[\s\S]*?>Unlock<\/h1>/);
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
    expect(infiniteAnimations.every((animation) => /loading-line|placement-|demo-pulse|cover-(?:atomic|character|caret)-cycle/.test(animation))).toBe(true);
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

  // Protects the 2026-08-06 deletion: the Write/Encrypt/Copy/Send stepper animated
  // one send mode's story on a screen offering four, so it is gone from main.ts
  // and every rule and keyframe it owned is gone from styles.css. This is a
  // deletion guard, not a motion check -- there is no such motion left to check.
  it("keeps the retired four-step sending demo out of both the app and the sheet", () => {
    expect(source).not.toContain("manualSendingAnimationMarkup");
    expect(source).not.toContain('step(1, "Write")');
    expect(source).not.toContain("manual-send-demo");
    expect(styles).not.toContain("manual-send-demo");
    expect(styles).not.toContain("manual-send-step");
    expect(styles).not.toContain("manual-send-flow");
  });

  // Protects: the two cover-insertion options are still told apart by looping
  // motion -- one box blinks in whole, the other types -- and reduced motion
  // still gets a COMPLETE static state, both boxes filled, so the comparison
  // survives without animation instead of showing two empty boxes.
  it("loops the explicit atomic and character comparison with a static reduced-motion state", () => {
    expect(coverSource).toContain('class="cover-demo-text cover-demo-atomic"');
    expect(coverSource).toContain('class="cover-demo-clip"');
    expect(coverSource).toContain('class="cover-caret"');
    expect(coverStyles).toContain("animation: cover-atomic 3.2s step-end infinite");
    expect(coverStyles).toContain("animation: cover-type 3s infinite");
    expect(coverStyles).toContain("animation: cover-caret-blink 1.1s step-end infinite");
    const reduced = coverStyles.slice(coverStyles.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(reduced).toContain(".cover-demo-atomic");
    expect(reduced).toContain(".cover-demo-clip");
    expect(reduced).toContain(".cover-caret { animation: none; }");
    expect(reduced).toContain(".cover-demo-atomic { opacity: 1; }");
    expect(reduced).toContain(".cover-demo-clip { width: 10ch; }");
    // Every loop in the module is one of those three; nothing else repeats.
    const loops = [...coverStyles.matchAll(/animation:\s*([^;]*\binfinite\b[^;]*);/gu)].map((match) => match[1]);
    expect(loops.length).toBe(3);
    expect(loops.every((loop) => /cover-(?:atomic|type|caret-blink)/u.test(loop))).toBe(true);
  });

  it("gives transient feedback an exit instead of abruptly removing it", () => {
    expect(source).toContain('toast.classList.add("toast-leaving")');
    expect(source).toContain('toast.addEventListener("animationend"');
    expect(styles).toContain("@keyframes toast-exit");
  });
});
