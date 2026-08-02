import { describe, expect, it, vi } from "vitest";
import {
  bindWindowLifecycleRealignment,
  type NativeWindowLifecycleEvents,
  type ResizeEventTarget,
  type VisibilityEventTarget,
} from "./window-lifecycle-bindings";

interface RegisteredHandlers {
  resize?: () => void;
  moved?: () => void;
  resized?: () => void;
  scaleChanged?: () => void;
  visibilitychange?: () => void;
}

function lifecycleFixture(visibilityState: "visible" | "hidden" = "visible"): {
  readonly handlers: RegisteredHandlers;
  readonly browserWindow: ResizeEventTarget;
  readonly desktopWindow: NativeWindowLifecycleEvents;
  readonly document: VisibilityEventTarget;
} {
  const handlers: RegisteredHandlers = {};
  return {
    handlers,
    browserWindow: {
      addEventListener: (_type, handler) => { handlers.resize = handler; },
    },
    desktopWindow: {
      onMoved: async (handler) => { handlers.moved = handler; },
      onResized: async (handler) => { handlers.resized = handler; },
      onScaleChanged: async (handler) => { handlers.scaleChanged = handler; },
    },
    document: {
      visibilityState,
      addEventListener: (_type, handler) => { handlers.visibilitychange = handler; },
    },
  };
}

describe("bindWindowLifecycleRealignment", () => {
  it("realigns for browser geometry, native move/resize, and DPI changes", async () => {
    const fixture = lifecycleFixture();
    const scheduleNativeHostRealignment = vi.fn();

    bindWindowLifecycleRealignment(
      fixture.browserWindow,
      fixture.desktopWindow,
      fixture.document,
      scheduleNativeHostRealignment,
    );
    await Promise.resolve();

    fixture.handlers.resize?.();
    fixture.handlers.moved?.();
    fixture.handlers.resized?.();
    fixture.handlers.scaleChanged?.();

    expect(scheduleNativeHostRealignment).toHaveBeenCalledTimes(4);
  });

  it("realigns when a minimised document is restored, but not when hidden", () => {
    const fixture = lifecycleFixture("hidden");
    const scheduleNativeHostRealignment = vi.fn();

    bindWindowLifecycleRealignment(
      fixture.browserWindow,
      fixture.desktopWindow,
      fixture.document,
      scheduleNativeHostRealignment,
    );
    fixture.handlers.visibilitychange?.();
    expect(scheduleNativeHostRealignment).not.toHaveBeenCalled();

    (fixture.document as { visibilityState: "visible" | "hidden" }).visibilityState = "visible";
    fixture.handlers.visibilitychange?.();
    expect(scheduleNativeHostRealignment).toHaveBeenCalledOnce();
  });

  it("reports listener-registration failures without leaving a rejected promise", async () => {
    const fixture = lifecycleFixture();
    const reportBindingFailure = vi.fn();
    fixture.desktopWindow.onScaleChanged = async () => { throw new Error("unavailable"); };

    bindWindowLifecycleRealignment(
      fixture.browserWindow,
      fixture.desktopWindow,
      fixture.document,
      vi.fn(),
      reportBindingFailure,
    );
    await Promise.resolve();

    expect(reportBindingFailure).toHaveBeenCalledOnce();
  });
});
