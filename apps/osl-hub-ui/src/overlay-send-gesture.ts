export type OverlaySendMode = "button" | "double" | "single";
export type OverlaySendGestureResult = "none" | "armed" | "send";

export interface OverlayEnterGesture {
  key: string;
  shiftKey: boolean;
  repeat: boolean;
  isTrusted: boolean;
  isComposing: boolean;
  now: number;
}

export class OverlaySendGesture {
  private mode: OverlaySendMode = "button";
  private enterDown = false;
  private armedUntil = 0;

  setMode(mode: OverlaySendMode): void {
    this.mode = mode;
    this.cancel();
  }

  cancel(): void {
    this.enterDown = false;
    this.armedUntil = 0;
  }

  keydown(event: OverlayEnterGesture): OverlaySendGestureResult {
    if (!event.isTrusted || event.isComposing || event.repeat || event.key !== "Enter" || event.shiftKey || this.mode === "button") return "none";
    if (this.enterDown) return "none";
    this.enterDown = true;
    if (this.mode === "single") return "send";
    return "none";
  }

  keyup(event: OverlayEnterGesture): OverlaySendGestureResult {
    if (!event.isTrusted || event.isComposing || event.repeat || event.key !== "Enter" || event.shiftKey || !this.enterDown) return "none";
    this.enterDown = false;
    if (this.mode !== "double") return "none";
    if (this.armedUntil >= event.now) {
      this.armedUntil = 0;
      return "send";
    }
    this.armedUntil = event.now + 1_200;
    return "armed";
  }

  expire(now: number): boolean {
    if (this.armedUntil === 0 || now < this.armedUntil) return false;
    this.cancel();
    return true;
  }
}
