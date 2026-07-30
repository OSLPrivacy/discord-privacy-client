import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import { bindMainWindowFocusChanges, RecoveryCaptureGate, type WindowFocusChangedEvent } from "./ui-behavior";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("recovery-secret capture ordering", () => {
  it("keeps recovery text and clipboard output behind the current proof latch", () => {
    expect(source).toContain("if (!recoveryCaptureGate.canRender()) return recoveryProtectionRefusalContent()");
    expect(source).toContain("if (!recoveryBundle || !recoveryCaptureGate.canRender()) return");
    expect(source).toContain("recoveryCaptureGate.invalidate()");
    expect(source).toContain("await proveRecoveryCaptureProtection()");
  });

  it("dispatches the real focus-loss seam and invalidates the accepted recovery proof", () => {
    const gate = new RecoveryCaptureGate();
    expect(gate.accept(gate.checkpoint())).toBe(true);
    expect(gate.canRender()).toBe(true);
    let screenshotProtectionEnabled = true;
    const render = vi.fn();
    const registered: { focusHandler?: (event: WindowFocusChangedEvent) => void } = {};

    void bindMainWindowFocusChanges(
      async (handler) => { registered.focusHandler = handler; },
      {
        scheduleNativeHostRealignment: vi.fn(),
        hasRecoverySecrets: () => true,
        proveRecoveryCaptureProtection: vi.fn(async () => true),
        invalidateRecoveryCapture: () => gate.invalidate(),
        setScreenshotProtectionEnabled: (enabled) => { screenshotProtectionEnabled = enabled; },
        render,
      },
    );
    expect(registered.focusHandler).toBeTypeOf("function");
    if (!registered.focusHandler) throw new Error("focus handler was not registered");
    registered.focusHandler({ payload: false });

    expect(gate.canRender()).toBe(false);
    expect(screenshotProtectionEnabled).toBe(false);
    expect(render).toHaveBeenCalledOnce();
  });

  it("wires the Tauri focus callback to the behavioural dispatcher", () => {
    const start = source.indexOf("void bindMainWindowFocusChanges");
    const end = source.indexOf('document.addEventListener("visibilitychange"', start);
    const focusBinding = source.slice(start, end);
    expect(focusBinding).toContain("(handler) => desktopWindow.onFocusChanged(handler)");
    expect(focusBinding).toContain("invalidateRecoveryCapture: () => recoveryCaptureGate.invalidate()");
  });

  it("starts unproven and names the protected refusal", () => {
    expect(source).toContain("let screenshotProtectionEnabled = false");
    expect(source).toContain("OSL cannot show recovery secrets because Windows capture resistance is not proven for this window");
  });

  it("gates additional-identity creation before the backend returns a new phrase", () => {
    const start = source.indexOf("async function createAdditionalIdentity");
    const end = source.indexOf("async function recoverAdditionalIdentity", start);
    const create = source.slice(start, end);
    expect(create.indexOf("await proveRecoveryCaptureProtection()")).toBeGreaterThanOrEqual(0);
    expect(create.indexOf("createHubIdentitySlot(label)")).toBeGreaterThan(create.indexOf("await proveRecoveryCaptureProtection()"));
    expect(create).toContain("if (!recoveryCaptureGate.canRender())");
  });
});
