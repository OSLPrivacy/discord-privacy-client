/**
 * The interaction half of the view-once overlay.
 *
 * The server claim is deliberately owned by Play, rather than by opening the
 * overlay or changing the duration control.  This keeps dismissal harmless:
 * closing the sheet ends only the local display session and never spends a
 * second server-side claim.
 */
import {
  viewOnceOverlayMarkup,
  type ViewOnceOverlayDurationChoice,
  type ViewOnceOverlayProtectedContent,
} from "./view-once-overlay";
import { createViewOnceTimer, type MonotonicNow, type ViewOnceTimer } from "./view-once-timer";

export interface ViewOnceOverlayClaim {
  readonly messageId: string;
  readonly content: ViewOnceOverlayProtectedContent;
  /** The duration authenticated by the claimed server record. */
  readonly displayDurationSeconds: number;
}

export interface ViewOnceDisplaySession {
  readonly durationSeconds: number;
  readonly timer: ViewOnceTimer;
}

export type ViewOnceOverlayPhase = "pending" | "claiming" | "displaying" | "closed";

export interface ViewOnceOverlaySnapshot {
  readonly phase: ViewOnceOverlayPhase;
  readonly selectedDurationSeconds: number;
  readonly displaySession: ViewOnceDisplaySession | null;
}

export interface ViewOnceOverlayControllerOptions {
  readonly messageId: string;
  readonly initialDurationSeconds: number;
  readonly durationChoices: readonly ViewOnceOverlayDurationChoice[];
  claim(messageId: string): Promise<ViewOnceOverlayClaim | null>;
  readonly now?: MonotonicNow;
  onClose?(): void;
}

export interface ViewOnceOverlayController {
  snapshot(): ViewOnceOverlaySnapshot;
  /** The duration control. A receiver may choose a shorter local display. */
  chooseDuration(seconds: number): boolean;
  /** Play is the sole route to the server claim. Repeated plays reuse it. */
  play(): Promise<void>;
  /** X: stop the local session. It never calls claim. */
  close(): void;
  /** Optional DOM binding for the markup produced by TASK 0564. */
  mount(root: HTMLElement, words: Record<string, string>): void;
}

function positiveWholeSeconds(seconds: number): boolean {
  return Number.isSafeInteger(seconds) && seconds >= 1 && seconds <= 60;
}

function includesChoice(choices: readonly ViewOnceOverlayDurationChoice[], seconds: number): boolean {
  return choices.some((choice) => choice.seconds === seconds);
}

/**
 * Make the view-once controls operational. The selected value can only shorten
 * the display: the signed server claim remains the upper bound. This lets a
 * person close sooner without allowing the overlay to extend a sender-selected
 * lifetime locally.
 */
export function createViewOnceOverlayController(options: ViewOnceOverlayControllerOptions): ViewOnceOverlayController {
  if (!options.messageId || !positiveWholeSeconds(options.initialDurationSeconds)
    || options.durationChoices.length === 0
    || !options.durationChoices.every((choice) => positiveWholeSeconds(choice.seconds))) {
    throw new Error("OSL: invalid view-once overlay session");
  }

  const choices = includesChoice(options.durationChoices, options.initialDurationSeconds)
    ? [...options.durationChoices]
    : [...options.durationChoices, { seconds: options.initialDurationSeconds }];
  let selectedDurationSeconds = options.initialDurationSeconds;
  let phase: ViewOnceOverlayPhase = "pending";
  let displaySession: ViewOnceDisplaySession | null = null;
  let content: ViewOnceOverlayProtectedContent | null = null;
  let claimPromise: Promise<void> | null = null;
  let displayTimeout: ReturnType<typeof setTimeout> | undefined;
  let root: HTMLElement | null = null;
  let words: Record<string, string> | null = null;
  let didClose = false;

  const finish = (): void => {
    if (didClose) return;
    didClose = true;
    if (displayTimeout !== undefined) clearTimeout(displayTimeout);
    displayTimeout = undefined;
    phase = "closed";
    displaySession = null;
    content = null;
    render();
    options.onClose?.();
  };

  const render = (): void => {
    if (!root || !words) return;
    root.innerHTML = viewOnceOverlayMarkup({
      open: phase !== "closed",
      // Pending and claiming states render a generic protected placeholder,
      // never plaintext that has not yet come back from the server claim.
      content,
      durationSeconds: selectedDurationSeconds,
      durationChoices: choices,
    }, words);
    const duration = root.querySelector<HTMLSelectElement>("[data-voo-duration]");
    if (duration) {
      duration.disabled = phase !== "pending";
      duration.addEventListener("change", () => {
        chooseDuration(Number(duration.value));
      });
    }
    root.querySelector<HTMLButtonElement>("[data-voo-play]")?.addEventListener("click", () => {
      void play();
    });
    root.querySelector<HTMLButtonElement>("[data-voo-close]")?.addEventListener("click", close);
  };

  const chooseDuration = (seconds: number): boolean => {
    if (phase !== "pending" || !includesChoice(choices, seconds)) return false;
    selectedDurationSeconds = seconds;
    render();
    return true;
  };

  const play = (): Promise<void> => {
    if (phase === "claiming") return claimPromise ?? Promise.resolve();
    if (phase !== "pending") return Promise.resolve();
    phase = "claiming";
    render();
    claimPromise = options.claim(options.messageId).then((claimed) => {
      if (!claimed || claimed.messageId !== options.messageId || !positiveWholeSeconds(claimed.displayDurationSeconds)) {
        if (phase !== "closed") {
          phase = "pending";
          render();
        }
        return;
      }
      if (phase === "closed") return;

      // The server record sets the maximum lifetime. The local duration choice
      // becomes the actual display session only when it is shorter.
      const durationSeconds = Math.min(selectedDurationSeconds, claimed.displayDurationSeconds);
      content = claimed.content;
      phase = "displaying";
      const timer = createViewOnceTimer({
        lifetimeMs: durationSeconds * 1_000,
        now: options.now,
        onClose: finish,
      });
      displaySession = { durationSeconds, timer };
      displayTimeout = setTimeout(() => timer.tick(), durationSeconds * 1_000);
      render();
    });
    return claimPromise;
  };

  const close = (): void => {
    if (phase === "displaying") displaySession?.timer.close();
    else finish();
  };

  return {
    snapshot: () => ({ phase, selectedDurationSeconds, displaySession }),
    chooseDuration,
    play,
    close,
    mount(nextRoot, nextWords) {
      root = nextRoot;
      words = nextWords;
      render();
    },
  };
}
