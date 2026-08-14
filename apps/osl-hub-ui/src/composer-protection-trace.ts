/**
 * Small, DOM-free state machine for the protected-composer border.
 *
 * A trace is an acknowledgement of one accepted off-to-on transition, not an
 * animation of every state announcement. The retained overlay receives repeat
 * state announcements during remeasure and restore, so the sequence number is
 * deliberately advanced only on a rising edge.
 */
export interface ComposerProtectionTraceState {
  readonly engaged: boolean;
  readonly traceCount: number;
}

export const NO_DISCORD_COMPOSER_REASON = "No Discord composer is detected.";

export interface ComposerLockAvailability {
  readonly unavailable: boolean;
  readonly disabled: boolean;
  readonly className: "" | " composer-unavailable";
  readonly tone: "normal" | "grey";
  readonly reason: string | null;
}

export function composerLockAvailability(
  composerDetected: boolean,
  protectionActive: boolean,
): ComposerLockAvailability {
  const unavailable = !composerDetected && !protectionActive;
  return {
    unavailable,
    disabled: unavailable,
    className: unavailable ? " composer-unavailable" : "",
    tone: unavailable ? "grey" : "normal",
    reason: unavailable ? NO_DISCORD_COMPOSER_REASON : null,
  };
}

export class ComposerProtectionTraceController {
  private engaged = false;
  private traceCount = 0;

  applyLockEngaged(lockEngaged: boolean, composerDetected: boolean): ComposerProtectionTraceState {
    const engaged = lockEngaged && composerDetected;
    if (engaged && !this.engaged) this.traceCount += 1;
    this.engaged = engaged;
    return this.snapshot();
  }

  snapshot(): ComposerProtectionTraceState {
    return { engaged: this.engaged, traceCount: this.traceCount };
  }
}
