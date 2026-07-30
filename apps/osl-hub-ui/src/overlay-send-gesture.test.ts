import { describe, expect, it } from "vitest";
import { OverlaySendGesture, SendOutcome, type OverlayEnterGesture } from "./overlay-send-gesture";

const enter = (now: number, overrides: Partial<OverlayEnterGesture> = {}): OverlayEnterGesture => ({
  key: "Enter", shiftKey: false, repeat: false, isTrusted: true, isComposing: false, now, ...overrides,
});

describe("native overlay send gestures", () => {
  it("keeps button mode and Shift+Enter multiline-only", () => {
    const gesture = new OverlaySendGesture();
    expect(gesture.keydown(enter(1))).toBe("none");
    gesture.setMode("single");
    expect(gesture.keydown(enter(2, { shiftKey: true }))).toBe("none");
  });

  it("leaves Backspace and Delete entirely to the native textarea editor", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("single");
    for (const key of ["Backspace", "Delete"]) {
      const editing = enter(1, { key });
      expect(gesture.keydown(editing)).toBe("none");
      expect(gesture.keyup(editing)).toBe("none");
    }
  });

  it("requires distinct trusted double-enter down/up gestures", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10))).toBe("none");
    expect(gesture.keydown(enter(11, { repeat: true }))).toBe("none");
    expect(gesture.keyup(enter(12))).toBe("armed");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("send");
  });

  it("does not let a held first Enter satisfy the second Enter", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10))).toBe("none");
    expect(gesture.keydown(enter(11))).toBe("none");
    expect(gesture.keydown(enter(12, { repeat: true }))).toBe("none");
    expect(gesture.keyup(enter(13))).toBe("armed");
    expect(gesture.keydown(enter(14, { repeat: true }))).toBe("none");
    expect(gesture.keyup(enter(15, { repeat: true }))).toBe("none");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("send");
  });

  it("d4 qualifies the Double Enter handoff as two release-completed gestures", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10))).toBe("none");
    expect(gesture.keydown(enter(11))).toBe("none");
    expect(gesture.keyup(enter(12))).toBe("armed");
    expect(gesture.keyup(enter(13))).toBe("none");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("send");
  });

  it("ignores synthetic input and expires without sending", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10, { isTrusted: false }))).toBe("none");
    expect(gesture.keyup(enter(11, { isTrusted: false }))).toBe("none");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("armed");
    expect(gesture.expire(1_221)).toBe(true);
    expect(gesture.keydown(enter(1_222))).toBe("none");
    expect(gesture.keyup(enter(1_223))).toBe("armed");
  });

  it("does not let an untrusted key release arm a trusted key press", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10))).toBe("none");
    expect(gesture.keyup(enter(11, { isTrusted: false }))).toBe("none");
    expect(gesture.keyup(enter(12))).toBe("none");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("armed");
  });

  it("requires a fresh trusted down before the second Enter can send", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10))).toBe("none");
    expect(gesture.keyup(enter(11))).toBe("armed");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21, { isTrusted: false }))).toBe("none");
    expect(gesture.keyup(enter(22))).toBe("none");
    expect(gesture.keydown(enter(30))).toBe("none");
    expect(gesture.keyup(enter(31))).toBe("send");
  });

  it("resets its armed state whenever mode changes", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    gesture.keydown(enter(10));
    expect(gesture.keyup(enter(11))).toBe("armed");
    gesture.setMode("single");
    gesture.setMode("double");
    expect(gesture.keydown(enter(12))).toBe("none");
    expect(gesture.keyup(enter(13))).toBe("armed");
  });
});

describe("native overlay send outcomes", () => {
  it("parses sent only from a fully verified sent receipt", () => {
    expect(SendOutcome.parse({ status: "sent", placed: true, enterSent: true })).toBe("sent");
    expect(SendOutcome.parse({ status: "sent", placed: true, enterSent: false })).toBe("unknown");
    expect(SendOutcome.parse("sent")).toBe("unknown");
  });

  it("parses explicit no-send terminal statuses as not-sent", () => {
    for (const status of [
      "calibrationRequired",
      "contextChanged",
      "composerUnavailable",
      "composerNotEmpty",
      "placementRejected",
      "enterRejected",
      "platformUnsupported",
    ]) {
      expect(SendOutcome.parse({ status, placed: false, enterSent: false })).toBe("not-sent");
      expect(SendOutcome.parse(status)).toBe("not-sent");
    }
  });

  it("keeps ambiguous carrier and malformed receipts unknown", () => {
    expect(SendOutcome.parse({ status: "carrierUnconfirmed", placed: true, enterSent: true })).toBe("unknown");
    expect(SendOutcome.parse({ status: "contextChanged", placed: true, enterSent: true })).toBe("unknown");
    expect(SendOutcome.parse({ status: "placementRejected", placed: "no", enterSent: false })).toBe("unknown");
    expect(SendOutcome.parse({ status: "newBackendStatus", placed: false, enterSent: false })).toBe("unknown");
    expect(SendOutcome.parse(null)).toBe("unknown");
  });
});
