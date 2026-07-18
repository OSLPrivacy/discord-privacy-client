export type FrameHandle = number;

type RequestFrame = (callback: FrameRequestCallback) => FrameHandle;
type CancelFrame = (handle: FrameHandle) => void;

/** Collapses any number of render requests into one commit per animation frame. */
export class FrameRenderScheduler {
  private pendingFrame: FrameHandle | null = null;

  constructor(
    private readonly requestFrame: RequestFrame,
    private readonly cancelFrame: CancelFrame,
    private readonly commit: () => void,
  ) {}

  request(): void {
    if (this.pendingFrame !== null) return;
    this.pendingFrame = this.requestFrame(() => {
      this.pendingFrame = null;
      this.commit();
    });
  }

  flush(): void {
    if (this.pendingFrame !== null) {
      this.cancelFrame(this.pendingFrame);
      this.pendingFrame = null;
    }
    this.commit();
  }

  cancel(): void {
    if (this.pendingFrame === null) return;
    this.cancelFrame(this.pendingFrame);
    this.pendingFrame = null;
  }
}
