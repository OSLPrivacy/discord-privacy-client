import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const source = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");

function between(start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  expect(from, `${start} should exist`).toBeGreaterThanOrEqual(0);
  expect(to, `${end} should follow ${start}`).toBeGreaterThan(from);
  return source.slice(from, to);
}

describe("native window transition settling", () => {
  it("arms a bounded resize listener before entering or leaving fullscreen", () => {
    const toggle = between(
      "async function toggleDesktopFullscreen",
      "async function focusActiveNativeCompanion",
    );
    expect(toggle.indexOf("waitForDesktopGeometrySettlement(appWindow)")).toBeLessThan(
      toggle.indexOf("await appWindow.setFullscreen(!fullscreen)"),
    );
    expect(toggle).toContain("await geometrySettled");
    expect(toggle.indexOf("await geometrySettled")).toBeLessThan(
      toggle.indexOf("resizeNativeAppWindow()"),
    );

    const settle = between(
      "function waitForDesktopGeometrySettlement",
      "async function toggleDesktopFullscreen",
    );
    expect(settle).toContain("appWindow.onResized(finish)");
    expect(settle).toContain("window.setTimeout(finish, timeoutMs)");
    expect(settle).toContain("requestAnimationFrame(() => resolve())");
    expect(settle).toContain("unlisten?.()");
  });

  it("coalesces busy transition events into a final validation pass", () => {
    const validation = between(
      "async function validateNativeSurfaces()",
      "function scheduleNativeHostRealignment",
    );
    expect(source).toContain("let nativeHostValidationPending = false;");
    expect(validation).toMatch(
      /if \(nativeHostValidationBusy\) \{\s*nativeHostValidationPending = true;\s*return;/,
    );
    expect(validation).toContain("nativeHostValidationPending = false;");
    expect(validation).toContain("await validateNativeSurfacesPass();");
    expect(validation).toContain("while (nativeHostValidationPending)");
    expect(validation.indexOf("nativeHostValidationPending = false;")).toBeLessThan(
      validation.indexOf("await validateNativeSurfacesPass();"),
    );
  });

  it("keeps one frame-level event ingress for move, resize, and focus", () => {
    const schedule = between(
      "function scheduleNativeHostRealignment",
      "window.addEventListener(\"resize\"",
    );
    expect(schedule).toContain("requestAnimationFrame");
    expect(schedule).toContain("void validateNativeSurfaces()");
    expect(source).toContain("desktopWindow.onMoved(scheduleNativeHostRealignment)");
    expect(source).toContain("desktopWindow.onResized(scheduleNativeHostRealignment)");
    expect(source).toContain("desktopWindow.onFocusChanged");
  });
});
