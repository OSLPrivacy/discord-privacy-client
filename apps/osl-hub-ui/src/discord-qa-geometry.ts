import type { NativeWindowHostAction } from "./services";

export const DISCORD_QA_GEOMETRY_INTERVAL_MS = 1_250;

export interface DiscordQaGeometryDependencies {
  isActive(): boolean;
  resize(): Promise<NativeWindowHostAction | null>;
  /**
   * Injected only by tests. This module owns the one bounded cadence the QA
   * carrier needs, so no caller has to hold a raw timer of its own to get it --
   * a timer a caller owns is a timer a caller can leak.
   */
  setInterval?(callback: () => void, delayMs: number): number;
  clearInterval?(handle: number): void;
}

export interface DiscordQaGeometryKeeper {
  start(): void;
  stop(): void;
  running(): boolean;
}

function isExactDiscordResize(receipt: NativeWindowHostAction | null): boolean {
  return receipt?.id === "discord"
    && receipt.status === "resized"
    && receipt.reason === "none"
    && receipt.mode === "existingNativeCompanion"
    && receipt.captureProtected === false;
}

/**
 * Keeps the disposable Discord QA carrier aligned without focusing, detaching,
 * reopening, or granting the renderer any new native-window authority.
 */
export function createDiscordQaGeometryKeeper(
  dependencies: DiscordQaGeometryDependencies,
): DiscordQaGeometryKeeper {
  let intervalHandle: number | null = null;
  let resizePending = false;
  let generation = 0;
  const startTimer = dependencies.setInterval
    ?? ((callback, delayMs) => window.setInterval(callback, delayMs));
  const stopTimer = dependencies.clearInterval
    ?? ((handle) => window.clearInterval(handle));

  const stop = (): void => {
    generation += 1;
    if (intervalHandle !== null) stopTimer(intervalHandle);
    intervalHandle = null;
  };

  const tick = async (expectedGeneration: number): Promise<void> => {
    if (expectedGeneration !== generation || !dependencies.isActive()) {
      stop();
      return;
    }
    if (resizePending) return;
    resizePending = true;
    try {
      const receipt = await dependencies.resize();
      if (expectedGeneration === generation && !isExactDiscordResize(receipt)) stop();
    } catch {
      if (expectedGeneration === generation) stop();
    } finally {
      resizePending = false;
    }
  };

  return {
    start(): void {
      if (!dependencies.isActive()) {
        stop();
        return;
      }
      if (intervalHandle !== null) return;
      generation += 1;
      const expectedGeneration = generation;
      intervalHandle = startTimer(
        () => { void tick(expectedGeneration); },
        DISCORD_QA_GEOMETRY_INTERVAL_MS,
      );
    },
    stop,
    running: () => intervalHandle !== null,
  };
}
