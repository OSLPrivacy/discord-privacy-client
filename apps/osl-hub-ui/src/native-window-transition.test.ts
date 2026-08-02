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
    // A caption drag delivers move events continuously and each pass is a
    // native round trip on the thread the modal drag loop already owns, so the
    // ingress must collapse them (dropped, never queued), pace consecutive
    // passes, and still guarantee the trailing one. That logic is a tested unit
    // -- native-realignment.test.ts owns the behavioural proofs -- and this
    // file only pins that main.ts routes through it instead of hand-rolling an
    // unpaced busy/pending loop again.
    const validation = between(
      "async function validateNativeSurfaces()",
      "function scheduleNativeHostRealignment",
    );
    expect(source).toContain('import { CoalescedRealignment, NativeCallGate } from "./native-realignment";');
    expect(source).toContain("const nativeHostRealignment = new CoalescedRealignment(validateNativeSurfacesPass);");
    expect(validation).toContain("await nativeHostRealignment.request();");
    expect(source).not.toContain("while (nativeHostValidationPending)");
  });

  it("gates every native geometry call so an abandoned one cannot multiply", () => {
    // withNativeDeadline abandons a slow call, it cannot cancel it: without the
    // gate each timed-out pass starts another invoke of the same command while
    // the first is still running in the backend.
    const pass = between(
      "async function validateNativeSurfacesPass",
      "const nativeHostRealignment",
    );
    expect(source).toContain("const nativeSurfaceCallGate = new NativeCallGate();");
    expect(source).toContain("await withNativeDeadline(nativeSurfaceCallGate.run(key, start), label, 3_000)");
    // A deadline is not an answer, so it must never reach a recovery branch.
    expect(source).toContain('return failure instanceof NativeDeadlineError ? "unanswered" : "failed";');
    for (const call of [
      "() => resizeNativeAppWindow()",
      "() => resizeDefaultBrowserCompanion()",
      "() => focusDefaultBrowserCompanion()",
      "() => resizeMullvadWindow()",
      "() => focusMullvadWindow()",
    ]) {
      expect(pass).toContain(call);
    }
    expect(pass).not.toContain("withNativeDeadline(");
  });

  it("keeps one frame-level event ingress for move, resize, scale, restore, and focus", () => {
    const schedule = between(
      "function scheduleNativeHostRealignment",
      "if (!runningUnderVitest)",
    );
    expect(schedule).toContain("requestAnimationFrame");
    expect(schedule).toContain("void validateNativeSurfaces()");
    expect(source).toContain('import { bindWindowLifecycleRealignment } from "./window-lifecycle-bindings"');
    expect(source).toContain("bindWindowLifecycleRealignment(\n    window,\n    desktopWindow,\n    document,\n    scheduleNativeHostRealignment,");
    expect(source).toContain("desktopWindow.onFocusChanged");
  });
});
