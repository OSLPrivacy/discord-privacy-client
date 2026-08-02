/**
 * The geometry signals which require borrowed native surfaces to be re-aligned.
 *
 * Focus remains bound with the recovery-capture dispatcher because focus loss
 * also changes capture-protection state. This module owns the signals whose
 * only shared effect is geometry realignment, including the events that are
 * easy to miss: a Tauri scale-factor change and restoring a hidden document.
 */
export interface NativeWindowLifecycleEvents {
  onMoved(handler: () => void): Promise<unknown>;
  onResized(handler: () => void): Promise<unknown>;
  onScaleChanged(handler: () => void): Promise<unknown>;
}

export interface ResizeEventTarget {
  addEventListener(type: "resize", handler: () => void): void;
}

export interface VisibilityEventTarget {
  readonly visibilityState: "visible" | "hidden" | string;
  addEventListener(type: "visibilitychange", handler: () => void): void;
}

/**
 * Bind every window-geometry signal to the coalesced realignment scheduler.
 *
 * A visibility transition is actionable on restore, not minimise: while hidden
 * a renderer frame can be paused, whereas becoming visible is the point at
 * which the borrowed surface must be brought back into the host's bounds.
 *
 * T7 can install this as one line:
 * `bindWindowLifecycleRealignment(window, desktopWindow, document, scheduleNativeHostRealignment);`
 */
export function bindWindowLifecycleRealignment(
  browserWindow: ResizeEventTarget,
  desktopWindow: NativeWindowLifecycleEvents,
  document: VisibilityEventTarget,
  scheduleNativeHostRealignment: () => void,
  reportBindingFailure: (error: unknown) => void = () => undefined,
): void {
  browserWindow.addEventListener("resize", scheduleNativeHostRealignment);
  void desktopWindow.onMoved(scheduleNativeHostRealignment).catch(reportBindingFailure);
  void desktopWindow.onResized(scheduleNativeHostRealignment).catch(reportBindingFailure);
  void desktopWindow.onScaleChanged(scheduleNativeHostRealignment).catch(reportBindingFailure);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") scheduleNativeHostRealignment();
  });
}
