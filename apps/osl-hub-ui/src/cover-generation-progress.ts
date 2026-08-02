/**
 * T7's rendering-facing view of the signal emitted by T13.  This deliberately
 * has no clock: only T13 can say whether generation is still making progress.
 */
export type CoverGenerationStage = "queued" | "generating" | "stalled";
export type CoverGenerationTerminalOutcome = "succeeded" | "fell-back" | "failed";

export type CoverGenerationSignal =
  | { readonly type: "stage"; readonly stage: CoverGenerationStage; readonly progress: number }
  | { readonly type: "terminal"; readonly outcome: CoverGenerationTerminalOutcome };

export type CoverGenerationPresentationState =
  | { readonly status: "idle" }
  | {
    readonly status: "generating";
    readonly stage: CoverGenerationStage;
    readonly progress: number;
    readonly isStalled: boolean;
    readonly progressAnimation: "running" | "stopped";
  }
  | { readonly status: "fell-back" }
  | { readonly status: "failed" };

export interface CoverGenerationProgressOptions {
  readonly onStateChange: (state: CoverGenerationPresentationState) => void;
}

/** Maps the T13 signal directly into the states rendered by T7. */
export class CoverGenerationProgress {
  private currentState: CoverGenerationPresentationState = { status: "idle" };

  constructor(private readonly options: CoverGenerationProgressOptions) {}

  get state(): CoverGenerationPresentationState {
    return this.currentState;
  }

  consume(signal: CoverGenerationSignal): void {
    if (signal.type === "terminal") {
      this.setState(terminalState(signal.outcome));
      return;
    }

    const isStalled = signal.stage === "stalled";
    this.setState({
      status: "generating",
      stage: signal.stage,
      progress: signal.progress,
      isStalled,
      progressAnimation: isStalled ? "stopped" : "running",
    });
  }

  private setState(state: CoverGenerationPresentationState): void {
    this.currentState = state;
    this.options.onStateChange(state);
  }
}

function terminalState(outcome: CoverGenerationTerminalOutcome): CoverGenerationPresentationState {
  switch (outcome) {
    case "succeeded": return { status: "idle" };
    case "fell-back": return { status: "fell-back" };
    case "failed": return { status: "failed" };
  }
}
