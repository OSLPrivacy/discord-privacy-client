export type OverlaySendMode = "button" | "double" | "single";
export type OverlaySendGestureResult = "none" | "armed" | "send";
export type SendOutcome = "sent" | "not-sent" | "unknown";

export const SendOutcome = Object.freeze({
  parse(value: unknown): SendOutcome {
    let status: string | null = null;
    if (typeof value === "string") {
      status = value;
    } else if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      const candidate = (value as Record<string, unknown>).status;
      status = typeof candidate === "string" ? candidate : null;
    }
    if (status === null) return "unknown";
    const notSentStatuses = [
      "calibrationRequired",
      "contextChanged",
      "composerUnavailable",
      "composerNotEmpty",
      "placementRejected",
      "enterRejected",
      "platformUnsupported",
    ];
    if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      const record = value as Record<string, unknown>;
      const placed = record.placed;
      const enterSent = record.enterSent;
      if ((placed !== undefined && typeof placed !== "boolean") || (enterSent !== undefined && typeof enterSent !== "boolean")) return "unknown";
      if (status === "sent") return placed === true && enterSent === true ? "sent" : "unknown";
      if (status === "carrierUnconfirmed" || enterSent === true) return "unknown";
      return notSentStatuses.includes(status) ? "not-sent" : "unknown";
    }
    if (status === "sent" || status === "carrierUnconfirmed") return "unknown";
    return notSentStatuses.includes(status) ? "not-sent" : "unknown";
  },
});

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
  private armedKeyReleased = false;

  setMode(mode: OverlaySendMode): void {
    this.mode = mode;
    this.cancel();
  }

  cancel(): void {
    this.enterDown = false;
    this.armedUntil = 0;
    this.armedKeyReleased = false;
  }

  keydown(event: OverlayEnterGesture): OverlaySendGestureResult {
    if (!event.isTrusted || event.isComposing || event.repeat || event.key !== "Enter" || event.shiftKey || this.mode === "button") return "none";
    if (this.enterDown) return "none";
    this.enterDown = true;
    if (this.mode === "single") return "send";
    if (this.armedUntil >= event.now && this.armedKeyReleased) {
      this.cancel();
      return "send";
    }
    this.armedUntil = event.now + 1_200;
    this.armedKeyReleased = false;
    return "armed";
  }

  keyup(event: OverlayEnterGesture): OverlaySendGestureResult {
    if (event.key !== "Enter" || !this.enterDown) return "none";
    this.enterDown = false;
    if (!event.isTrusted || event.isComposing || event.repeat || event.shiftKey) return "none";
    if (this.mode !== "double") return "none";
    if (this.armedUntil < event.now) {
      this.cancel();
      return "none";
    }
    this.armedKeyReleased = true;
    return "armed";
  }

  expire(now: number): boolean {
    if (this.armedUntil === 0 || now < this.armedUntil) return false;
    this.cancel();
    return true;
  }
}
