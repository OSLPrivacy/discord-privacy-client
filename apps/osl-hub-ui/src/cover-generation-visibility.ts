/**
 * Presentation timing policy for cover-text generation.
 *
 * A warm pool should complete before a person sees any progress affordance. If
 * it does not, keep the affordance visible long enough to read rather than
 * flashing it for a frame. D74 delegates these technical thresholds; they are
 * deliberately exported so a later measurement can revise them in one place.
 */
export const COVER_GENERATION_APPEAR_DELAY_MS = 400;
export const COVER_GENERATION_MIN_VISIBLE_MS = 600;

type Timer = ReturnType<typeof globalThis.setTimeout>;

export interface CoverGenerationVisibilityOptions {
  readonly onVisibilityChange: (visible: boolean) => void;
  readonly now?: () => number;
  readonly schedule?: (callback: () => void, delayMs: number) => Timer;
  readonly cancel?: (timer: Timer) => void;
}

/**
 * Turns generation start/finish events into a stable visible state. It does
 * not create DOM, styles, or progress; T7-82 supplies the signal and renderer.
 */
export class CoverGenerationVisibility {
  private readonly now: () => number;
  private readonly schedule: (callback: () => void, delayMs: number) => Timer;
  private readonly cancel: (timer: Timer) => void;
  private generationActive = false;
  private shownAt: number | null = null;
  private showTimer: Timer | null = null;
  private hideTimer: Timer | null = null;

  constructor(private readonly options: CoverGenerationVisibilityOptions) {
    this.now = options.now ?? Date.now;
    this.schedule = options.schedule ?? ((callback, delayMs) => globalThis.setTimeout(callback, delayMs));
    this.cancel = options.cancel ?? ((timer) => globalThis.clearTimeout(timer));
  }

  get visible(): boolean {
    return this.shownAt !== null;
  }

  start(): void {
    this.generationActive = true;
    this.clearHideTimer();
    if (this.visible || this.showTimer !== null) return;

    this.showTimer = this.schedule(() => {
      this.showTimer = null;
      if (!this.generationActive || this.visible) return;
      this.shownAt = this.now();
      this.options.onVisibilityChange(true);
    }, COVER_GENERATION_APPEAR_DELAY_MS);
  }

  finish(): void {
    this.generationActive = false;
    this.clearShowTimer();
    if (this.shownAt === null) return;

    const remainingMs = COVER_GENERATION_MIN_VISIBLE_MS - (this.now() - this.shownAt);
    if (remainingMs <= 0) {
      this.hide();
      return;
    }
    this.hideTimer = this.schedule(() => this.hide(), remainingMs);
  }

  dispose(): void {
    this.generationActive = false;
    this.clearShowTimer();
    this.clearHideTimer();
    if (this.visible) this.hide();
  }

  private hide(): void {
    this.clearHideTimer();
    if (this.shownAt === null) return;
    this.shownAt = null;
    this.options.onVisibilityChange(false);
  }

  private clearShowTimer(): void {
    if (this.showTimer === null) return;
    this.cancel(this.showTimer);
    this.showTimer = null;
  }

  private clearHideTimer(): void {
    if (this.hideTimer === null) return;
    this.cancel(this.hideTimer);
    this.hideTimer = null;
  }
}
